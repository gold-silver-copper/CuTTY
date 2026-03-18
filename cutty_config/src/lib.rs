use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

use log::LevelFilter;
use serde::Deserialize;
use toml::Value;

pub trait SerdeReplace {
    fn replace(&mut self, value: Value) -> Result<(), Box<dyn Error>>;
}

#[macro_export]
macro_rules! impl_replace {
    ($($ty:ty),*$(,)*) => {
        $(
            impl SerdeReplace for $ty {
                fn replace(&mut self, value: Value) -> Result<(), Box<dyn Error>> {
                    replace_simple(self, value)
                }
            }
        )*
    };
}

#[rustfmt::skip]
impl_replace!(
    usize, u8, u16, u32, u64, u128,
    isize, i8, i16, i32, i64, i128,
    f32, f64,
    bool,
    char,
    String,
    PathBuf,
    LevelFilter,
);

fn replace_simple<'de, D>(data: &mut D, value: Value) -> Result<(), Box<dyn Error>>
where
    D: Deserialize<'de>,
{
    *data = D::deserialize(value)?;

    Ok(())
}

impl<'de, T: Deserialize<'de>> SerdeReplace for Vec<T> {
    fn replace(&mut self, value: Value) -> Result<(), Box<dyn Error>> {
        replace_simple(self, value)
    }
}

impl<'de, T: SerdeReplace + Deserialize<'de>> SerdeReplace for Option<T> {
    fn replace(&mut self, value: Value) -> Result<(), Box<dyn Error>> {
        match self {
            Some(inner) => inner.replace(value),
            None => replace_simple(self, value),
        }
    }
}

impl<'de, T: Deserialize<'de>> SerdeReplace for HashMap<String, T> {
    fn replace(&mut self, value: Value) -> Result<(), Box<dyn Error>> {
        // Deserialize replacement as HashMap.
        let hashmap: HashMap<String, T> = Self::deserialize(value)?;

        // Merge the two HashMaps, replacing existing values.
        for (key, value) in hashmap {
            self.insert(key, value);
        }

        Ok(())
    }
}
