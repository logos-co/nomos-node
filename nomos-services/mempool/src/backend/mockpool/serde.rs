use std::hash::Hash;

use const_hex::{FromHex, ToHexExt};
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
    Key: Eq + Hash + ToHexExt,
    Item: Serialize + Clone,
    Serializer: SerializerTrait,
{
    items
        .into_iter()
        .map(|(k, v)| (k.encode_hex(), v.clone()))
        .collect::<Vec<(String, Item)>>()
        .serialize(serializer)
}

pub(super) fn deserialize_pending_items<'de, Key, Item, Deserializer>(
    deserializer: Deserializer,
) -> Result<LinkedHashMap<Key, Item>, Deserializer::Error>
where
    Key: Eq + Hash + FromHex,
    Item: DeserializeOwned,
    Deserializer: DeserializerTrait<'de>,
{
    Vec::<(String, Item)>::deserialize(deserializer).map(|items| {
        items
            .into_iter()
            .map(|(k, v)| {
                let Ok(decoded_key) = Key::from_hex(k) else {
                    panic!("Failed to decode serialized key.")
                };
                (decoded_key, v)
            })
            .collect::<LinkedHashMap<Key, Item>>()
    })
}
