use std::fmt::{self, Display};

use serde::{__private::ser::FlatMapSerializer, Serialize, Serializer, ser::SerializeMap};
#[cfg(feature = "simdnbt")]
use simdnbt::Serialize as _;

use crate::{
    FormattedText, base_component::BaseComponent, style::Style, text_component::TextComponent,
};

#[derive(Clone, Debug, PartialEq, Serialize, Eq, Hash)]
#[serde(untagged)]
pub enum StringOrComponent {
    String(String),
    FormattedText(FormattedText),
}

#[cfg(feature = "simdnbt")]
impl simdnbt::ToNbtTag for StringOrComponent {
    fn to_nbt_tag(self) -> simdnbt::owned::NbtTag {
        match self {
            StringOrComponent::String(s) => s.to_nbt_tag(),
            StringOrComponent::FormattedText(c) => c.to_nbt_tag(),
        }
    }
}

/// A message whose content depends on the client's language.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TranslatableComponent {
    pub base: BaseComponent,
    pub key: String,
    pub args: Vec<StringOrComponent>,
}

impl Serialize for TranslatableComponent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("translate", &self.key)?;
        Serialize::serialize(&self.base, FlatMapSerializer(&mut state))?;
        state.serialize_entry("with", &self.args)?;
        state.end()
    }
}

#[cfg(feature = "simdnbt")]
fn serialize_args_as_nbt(args: &[StringOrComponent]) -> simdnbt::owned::NbtList {
    // if it's all strings then make it a string list
    // if it's all components then make it a compound list
    // if it's a mix then return an error

    use tracing::debug;

    let mut string_list = Vec::new();
    let mut compound_list = Vec::new();

    for arg in args {
        match arg {
            StringOrComponent::String(s) => {
                string_list.push(s.clone());
            }
            StringOrComponent::FormattedText(c) => {
                compound_list.push(c.clone().to_compound());
            }
        }
    }

    if !string_list.is_empty() && !compound_list.is_empty() {
        // i'm actually not sure what vanilla does here, so i just made it return the
        // string list
        debug!("Tried to serialize a TranslatableComponent with a mix of strings and components.");
        return string_list.into();
    }

    if !string_list.is_empty() {
        return string_list.into();
    }

    compound_list.into()
}

#[cfg(feature = "simdnbt")]
impl simdnbt::Serialize for TranslatableComponent {
    fn to_compound(self) -> simdnbt::owned::NbtCompound {
        let mut compound = simdnbt::owned::NbtCompound::new();
        compound.insert("translate", self.key);
        compound.extend(self.base.style.to_compound());

        compound.insert("with", serialize_args_as_nbt(&self.args));
        compound
    }
}

impl TranslatableComponent {
    pub fn new(key: String, args: Vec<StringOrComponent>) -> Self {
        Self {
            base: BaseComponent::new(),
            key,
            args,
        }
    }

    /// Convert the key and args to a FormattedText.
    pub fn read(&self) -> Result<TextComponent, fmt::Error> {
        let template = azalea_language::get(&self.key).unwrap_or(&self.key);
        // decode the % things

        let mut i = 0;
        let mut matched = 0;

        // every time we get a char we add it to built_text, and we push it to
        // `arguments` and clear it when we add a new argument component
        let mut built_text = String::new();
        let mut components = Vec::new();

        while i < template.chars().count() {
            if template.chars().nth(i).unwrap() == '%' {
                let Some(char_after) = template.chars().nth(i + 1) else {
                    built_text.push('%');
                    break;
                };
                i += 1;
                match char_after {
                    '%' => {
                        built_text.push('%');
                    }
                    's' => {
                        let arg_component = self
                            .args
                            .get(matched)
                            .cloned()
                            .unwrap_or_else(|| StringOrComponent::String("".to_string()));

                        components.push(TextComponent::new(built_text.clone()));
                        built_text.clear();
                        components.push(TextComponent::from(arg_component));
                        matched += 1;
                    }
                    _ => {
                        // check if the char is a number
                        if let Some(d) = char_after.to_digit(10) {
                            // make sure the next two chars are $s
                            if let Some('$') = template.chars().nth(i + 1) {
                                if let Some('s') = template.chars().nth(i + 2) {
                                    i += 2;
                                    built_text.push_str(
                                        &self
                                            .args
                                            .get((d - 1) as usize)
                                            .unwrap_or(&StringOrComponent::String("".to_string()))
                                            .to_string(),
                                    );
                                } else {
                                    return Err(fmt::Error);
                                }
                            } else {
                                return Err(fmt::Error);
                            }
                        } else {
                            i -= 1;
                            built_text.push('%');
                        }
                    }
                }
            } else {
                built_text.push(template.chars().nth(i).unwrap());
            }

            i += 1;
        }

        if components.is_empty() {
            return Ok(TextComponent::new(built_text));
        }

        components.push(TextComponent::new(built_text));

        Ok(TextComponent {
            base: BaseComponent {
                siblings: components.into_iter().map(FormattedText::Text).collect(),
                style: Style::default(),
            },
            text: "".to_string(),
        })
    }
}

impl Display for TranslatableComponent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // this contains the final string will all the ansi escape codes
        for component in FormattedText::Translatable(self.clone()).into_iter() {
            let component_text = match &component {
                FormattedText::Text(c) => c.text.to_string(),
                FormattedText::Translatable(c) => match c.read() {
                    Ok(c) => c.to_string(),
                    Err(_) => c.key.to_string(),
                },
            };

            f.write_str(&component_text)?;
        }

        Ok(())
    }
}

impl Display for StringOrComponent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            StringOrComponent::String(s) => write!(f, "{s}"),
            StringOrComponent::FormattedText(c) => write!(f, "{c}"),
        }
    }
}

impl From<StringOrComponent> for TextComponent {
    fn from(soc: StringOrComponent) -> Self {
        match soc {
            StringOrComponent::String(s) => TextComponent::new(s),
            StringOrComponent::FormattedText(c) => TextComponent::new(c.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_none() {
        let c = TranslatableComponent::new("translation.test.none".to_string(), vec![]);
        assert_eq!(c.read().unwrap().to_string(), "Hello, world!".to_string());
    }
    #[test]
    fn test_complex() {
        let c = TranslatableComponent::new(
            "translation.test.complex".to_string(),
            vec![
                StringOrComponent::String("a".to_string()),
                StringOrComponent::String("b".to_string()),
                StringOrComponent::String("c".to_string()),
                StringOrComponent::String("d".to_string()),
            ],
        );
        // so true mojang
        assert_eq!(
            c.read().unwrap().to_string(),
            "Prefix, ab again b and a lastly c and also a again!".to_string()
        );
    }
    #[test]
    fn test_escape() {
        let c = TranslatableComponent::new(
            "translation.test.escape".to_string(),
            vec![
                StringOrComponent::String("a".to_string()),
                StringOrComponent::String("b".to_string()),
                StringOrComponent::String("c".to_string()),
                StringOrComponent::String("d".to_string()),
            ],
        );
        assert_eq!(c.read().unwrap().to_string(), "%s %a %%s %%b".to_string());
    }
    #[test]
    fn test_invalid() {
        let c = TranslatableComponent::new(
            "translation.test.invalid".to_string(),
            vec![
                StringOrComponent::String("a".to_string()),
                StringOrComponent::String("b".to_string()),
                StringOrComponent::String("c".to_string()),
                StringOrComponent::String("d".to_string()),
            ],
        );
        assert_eq!(c.read().unwrap().to_string(), "hi %".to_string());
    }
    #[test]
    fn test_invalid2() {
        let c = TranslatableComponent::new(
            "translation.test.invalid2".to_string(),
            vec![
                StringOrComponent::String("a".to_string()),
                StringOrComponent::String("b".to_string()),
                StringOrComponent::String("c".to_string()),
                StringOrComponent::String("d".to_string()),
            ],
        );
        assert_eq!(c.read().unwrap().to_string(), "hi %  s".to_string());
    }
}
