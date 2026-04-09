use serde::ser::Error as SerError;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::HashMap;

pub fn serialize_optional_map_as_object<S, T>(
    value: &Option<HashMap<String, T>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    match value {
        Some(map) => map.serialize(serializer),
        None => HashMap::<String, T>::new().serialize(serializer),
    }
}

pub fn is_false(value: &bool) -> bool {
    !value
}

pub fn insert_json<S, E>(map: &mut Map<String, Value>, key: &str, value: S) -> Result<(), E>
where
    S: Serialize,
    E: SerError,
{
    map.insert(
        key.to_string(),
        serde_json::to_value(value).map_err(E::custom)?,
    );
    Ok(())
}
