use serde::Deserialize;
use serde::Deserializer;

pub fn tags_deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let str_sequence = Option::<String>::deserialize(deserializer)?;
    if let Some(string) = str_sequence {
        Ok(Some(string.split(',').map(|t| t.to_owned()).collect()))
    } else {
        Ok(None)
    }
}
