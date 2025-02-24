use std::hash::Hash;

use linked_hash_map::LinkedHashMap;
use serde::{
    de::DeserializeOwned, Deserialize, Deserializer as DeserializerTrait, Serialize,
    Serializer as SerializerTrait,
};

pub(super) fn serialize_pending_items<Key, Item, Serializer>(
    items: &LinkedHashMap<Key, Item>,
    serializer: Serializer,
) -> Result<Serializer::Ok, Serializer::Error>
where
    Key: Eq + Hash + AsRef<[u8]>,
    Item: Serialize + Clone,
    Serializer: SerializerTrait,
{
    items
        .into_iter()
        .map(|(k, v)| (hex::encode(k), v.clone()))
        .collect::<Vec<(String, Item)>>()
        .serialize(serializer)
}

pub(super) fn deserialize_pending_items<'de, Key, Item, Deserializer>(
    deserializer: Deserializer,
) -> Result<LinkedHashMap<Key, Item>, Deserializer::Error>
where
    Key: Eq + Hash + TryFrom<Vec<u8>>,
    Item: DeserializeOwned,
    Deserializer: DeserializerTrait<'de>,
{
    Vec::<(String, Item)>::deserialize(deserializer).map(|items| {
        items
            .into_iter()
            .map(|(k, v)| {
                let Ok(decoded_key) = hex::decode(k)
                    .expect("Failed to decode serialized key.")
                    .try_into()
                else {
                    panic!("Failed to convert deserialized key into expected Key.");
                };
                (decoded_key, v)
            })
            .collect::<LinkedHashMap<Key, Item>>()
    })
}
