//! A resource, like minecraft:stone

use std::{
    fmt,
    io::{self, Cursor, Write},
    str::FromStr,
};

use azalea_buf::{AzaleaRead, AzaleaWrite, BufReadError};
#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use simdnbt::{FromNbtTag, ToNbtTag, owned::NbtTag};

#[derive(Hash, Clone, PartialEq, Eq)]
pub struct ResourceLocation {
    pub namespace: String,
    pub path: String,
}

static DEFAULT_NAMESPACE: &str = "minecraft";
// static REALMS_NAMESPACE: &str = "realms";

impl ResourceLocation {
    pub fn new(resource_string: &str) -> ResourceLocation {
        let sep_byte_position_option = resource_string.chars().position(|c| c == ':');
        let (namespace, path) = if let Some(sep_byte_position) = sep_byte_position_option {
            if sep_byte_position == 0 {
                (DEFAULT_NAMESPACE, &resource_string[1..])
            } else {
                (
                    &resource_string[..sep_byte_position],
                    &resource_string[sep_byte_position + 1..],
                )
            }
        } else {
            (DEFAULT_NAMESPACE, resource_string)
        };
        ResourceLocation {
            namespace: namespace.to_string(),
            path: path.to_string(),
        }
    }
}

impl fmt::Display for ResourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}
impl fmt::Debug for ResourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}
impl FromStr for ResourceLocation {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ResourceLocation::new(s))
    }
}

impl AzaleaRead for ResourceLocation {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let location_string = String::azalea_read(buf)?;
        Ok(ResourceLocation::new(&location_string))
    }
}
impl AzaleaWrite for ResourceLocation {
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()> {
        self.to_string().azalea_write(buf)
    }
}

#[cfg(feature = "serde")]
impl Serialize for ResourceLocation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for ResourceLocation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s.contains(':') {
            Ok(ResourceLocation::new(&s))
        } else {
            Err(de::Error::invalid_value(
                de::Unexpected::Str(&s),
                &"a valid ResourceLocation",
            ))
        }
    }
}

impl FromNbtTag for ResourceLocation {
    fn from_nbt_tag(tag: simdnbt::borrow::NbtTag) -> Option<Self> {
        tag.string().and_then(|s| s.to_str().parse().ok())
    }
}

impl ToNbtTag for ResourceLocation {
    fn to_nbt_tag(self) -> NbtTag {
        NbtTag::String(self.to_string().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_resource_location() {
        let r = ResourceLocation::new("abcdef:ghijkl");
        assert_eq!(r.namespace, "abcdef");
        assert_eq!(r.path, "ghijkl");
    }
    #[test]
    fn no_namespace() {
        let r = ResourceLocation::new("azalea");
        assert_eq!(r.namespace, "minecraft");
        assert_eq!(r.path, "azalea");
    }
    #[test]
    fn colon_start() {
        let r = ResourceLocation::new(":azalea");
        assert_eq!(r.namespace, "minecraft");
        assert_eq!(r.path, "azalea");
    }
    #[test]
    fn colon_end() {
        let r = ResourceLocation::new("azalea:");
        assert_eq!(r.namespace, "azalea");
        assert_eq!(r.path, "");
    }

    #[test]
    fn azbuf_resource_location() {
        let mut buf = Vec::new();
        ResourceLocation::new("minecraft:dirt")
            .azalea_write(&mut buf)
            .unwrap();

        let mut buf = Cursor::new(&buf[..]);

        assert_eq!(
            ResourceLocation::azalea_read(&mut buf).unwrap(),
            ResourceLocation::new("minecraft:dirt")
        );
    }
}
