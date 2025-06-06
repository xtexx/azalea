use std::{
    fmt::{self, Debug},
    io::{self, Cursor, Write},
};

use azalea_buf::{AzaleaRead, AzaleaWrite, BufReadError};

#[derive(Hash, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    PEACEFUL = 0,
    EASY = 1,
    NORMAL = 2,
    HARD = 3,
}

pub enum Err {
    InvalidDifficulty(String),
}

impl Debug for Err {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Err::InvalidDifficulty(s) => write!(f, "Invalid difficulty: {s}"),
        }
    }
}

impl Difficulty {
    pub fn name(&self) -> &'static str {
        match self {
            Difficulty::PEACEFUL => "peaceful",
            Difficulty::EASY => "easy",
            Difficulty::NORMAL => "normal",
            Difficulty::HARD => "hard",
        }
    }

    pub fn from_name(name: &str) -> Result<Difficulty, Err> {
        match name {
            "peaceful" => Ok(Difficulty::PEACEFUL),
            "easy" => Ok(Difficulty::EASY),
            "normal" => Ok(Difficulty::NORMAL),
            "hard" => Ok(Difficulty::HARD),
            _ => Err(Err::InvalidDifficulty(name.to_string())),
        }
    }

    pub fn by_id(id: u8) -> Difficulty {
        match id % 4 {
            0 => Difficulty::PEACEFUL,
            1 => Difficulty::EASY,
            2 => Difficulty::NORMAL,
            3 => Difficulty::HARD,
            // this shouldn't be possible because of the modulo, so panicking is fine
            _ => panic!("Unknown difficulty id: {id}"),
        }
    }

    pub fn id(&self) -> u8 {
        match self {
            Difficulty::PEACEFUL => 0,
            Difficulty::EASY => 1,
            Difficulty::NORMAL => 2,
            Difficulty::HARD => 3,
        }
    }
}

impl AzaleaRead for Difficulty {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(Difficulty::by_id(u8::azalea_read(buf)?))
    }
}

impl AzaleaWrite for Difficulty {
    fn azalea_write(&self, buf: &mut impl Write) -> io::Result<()> {
        u8::azalea_write(&self.id(), buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_difficulty_from_name() {
        assert_eq!(
            Difficulty::PEACEFUL,
            Difficulty::from_name("peaceful").unwrap()
        );
        assert_eq!(Difficulty::EASY, Difficulty::from_name("easy").unwrap());
        assert_eq!(Difficulty::NORMAL, Difficulty::from_name("normal").unwrap());
        assert_eq!(Difficulty::HARD, Difficulty::from_name("hard").unwrap());
        assert!(Difficulty::from_name("invalid").is_err());
    }

    #[test]
    fn test_difficulty_id() {
        assert_eq!(0, Difficulty::PEACEFUL.id());
        assert_eq!(1, Difficulty::EASY.id());
        assert_eq!(2, Difficulty::NORMAL.id());
        assert_eq!(3, Difficulty::HARD.id());
    }

    #[test]
    fn test_difficulty_name() {
        assert_eq!("peaceful", Difficulty::PEACEFUL.name());
        assert_eq!("easy", Difficulty::EASY.name());
        assert_eq!("normal", Difficulty::NORMAL.name());
        assert_eq!("hard", Difficulty::HARD.name());
    }
}
