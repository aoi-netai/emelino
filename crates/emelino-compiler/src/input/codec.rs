//! Stable integer IDs and lossless values for the owned backend input format.
use la_arena::{Arena, Idx};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod index {
    use super::*;
    pub fn serialize<S: Serializer, T>(id: &Idx<T>, serializer: S) -> Result<S::Ok, S::Error> {
        u32::from(id.into_raw()).serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>, T>(deserializer: D) -> Result<Idx<T>, D::Error> {
        u32::deserialize(deserializer).map(|raw| Idx::from_raw(raw.into()))
    }
}

pub mod indices {
    use super::*;
    pub fn serialize<S: Serializer, T>(ids: &[Idx<T>], serializer: S) -> Result<S::Ok, S::Error> {
        ids.iter()
            .map(|id| u32::from(id.into_raw()))
            .collect::<Vec<_>>()
            .serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>, T>(
        deserializer: D,
    ) -> Result<Vec<Idx<T>>, D::Error> {
        Vec::<u32>::deserialize(deserializer).map(|ids| {
            ids.into_iter()
                .map(|raw| Idx::from_raw(raw.into()))
                .collect()
        })
    }
}

pub mod optional_index {
    use super::*;
    pub fn serialize<S: Serializer, T>(
        id: &Option<Idx<T>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        id.map(|id| u32::from(id.into_raw())).serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>, T>(
        deserializer: D,
    ) -> Result<Option<Idx<T>>, D::Error> {
        Option::<u32>::deserialize(deserializer).map(|id| id.map(|raw| Idx::from_raw(raw.into())))
    }
}

pub mod index_pairs {
    use super::*;
    type Pairs<A, B> = Vec<(Idx<A>, Idx<B>)>;
    pub fn serialize<S: Serializer, A, B>(
        ids: &[(Idx<A>, Idx<B>)],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        ids.iter()
            .map(|(a, b)| (u32::from(a.into_raw()), u32::from(b.into_raw())))
            .collect::<Vec<_>>()
            .serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>, A, B>(
        deserializer: D,
    ) -> Result<Pairs<A, B>, D::Error> {
        Vec::<(u32, u32)>::deserialize(deserializer).map(|ids| {
            ids.into_iter()
                .map(|(a, b)| (Idx::from_raw(a.into()), Idx::from_raw(b.into())))
                .collect()
        })
    }
}

pub mod index_groups {
    use super::*;
    pub fn serialize<S: Serializer, T>(
        ids: &[Vec<Idx<T>>],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        ids.iter()
            .map(|group| {
                group
                    .iter()
                    .map(|id| u32::from(id.into_raw()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
            .serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>, T>(
        deserializer: D,
    ) -> Result<Vec<Vec<Idx<T>>>, D::Error> {
        Vec::<Vec<u32>>::deserialize(deserializer).map(|groups| {
            groups
                .into_iter()
                .map(|ids| {
                    ids.into_iter()
                        .map(|raw| Idx::from_raw(raw.into()))
                        .collect()
                })
                .collect()
        })
    }
}

pub mod arena {
    use super::*;
    pub fn serialize<S: Serializer, T: Serialize>(
        arena: &Arena<T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        arena.values().collect::<Vec<_>>().serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<Arena<T>, D::Error> {
        Vec::<T>::deserialize(deserializer).map(|values| values.into_iter().collect())
    }
}

pub mod float_bits {
    use super::*;
    pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
        value.to_bits().serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
        u64::deserialize(deserializer).map(f64::from_bits)
    }
}
