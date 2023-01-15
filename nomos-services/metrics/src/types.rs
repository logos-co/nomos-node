use ::core::ops::{Deref, DerefMut};
#[cfg(feature = "async-graphql")]
use async_graphql::{parser::types::Field, ContextSelectionSet, Positioned, ServerResult, Value};
use prometheus::HistogramOpts;
pub use prometheus::{
    core::{self, Atomic, GenericCounter as PrometheusGenericCounter},
    labels, opts, Opts,
};
use serde::{
    de::{DeserializeOwned, MapAccess, Visitor},
    ser::{SerializeStruct, SerializeTupleStruct},
    Deserialize, Serialize,
};

use crate::GraphqlData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsData {
    ty: MetricDataType,
    id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseMetricsDataError {
    #[error("fail to encode metrics data: {0}")]
    EncodeError(serde_json::Error),
    #[error("fail to decode metrics data: {0}")]
    DecodeError(serde_json::Error),
}

#[cfg(feature = "gql")]
impl GraphqlData for MetricsData {
    type Error = ParseMetricsDataError;

    fn try_from_bytes(src: &[u8]) -> Result<Self, Self::Error> {
        serde_json::from_slice(src).map_err(Self::Error::DecodeError)
    }

    fn try_into_bytes(self) -> Result<bytes::Bytes, Self::Error> {
        serde_json::to_vec(&self)
            .map(bytes::Bytes::from)
            .map_err(Self::Error::EncodeError)
    }
}

impl MetricsData {
    #[inline]
    pub fn new(ty: MetricDataType, id: String) -> Self {
        Self { ty, id }
    }

    #[inline]
    pub fn ty(&self) -> &MetricDataType {
        &self.ty
    }

    #[inline]
    pub fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricDataType {
    IntCounter(IntCounter),
    Counter(Counter),
    IntGauge(IntGauge),
    Gauge(Gauge),
    Histogram(Histogram),
}

#[cfg(feature = "async-graphql")]
#[async_trait::async_trait]
impl async_graphql::OutputType for MetricDataType {
    fn type_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("MetricDataType")
    }

    fn create_type_info(registry: &mut async_graphql::registry::Registry) -> String {
        registry.create_output_type::<Self, _>(
            async_graphql::registry::MetaTypeId::Union,
            |registry| async_graphql::registry::MetaType::Union {
                name: Self::type_name().to_string(),
                description: None,
                possible_types: {
                    let mut map = async_graphql::indexmap::IndexSet::new();
                    map.insert(<IntCounter as async_graphql::OutputType>::create_type_info(
                        registry,
                    ));
                    map.insert(<Counter as async_graphql::OutputType>::create_type_info(
                        registry,
                    ));
                    map.insert(<IntGauge as async_graphql::OutputType>::create_type_info(
                        registry,
                    ));
                    map.insert(<Gauge as async_graphql::OutputType>::create_type_info(
                        registry,
                    ));
                    map.insert(<Histogram as async_graphql::OutputType>::create_type_info(
                        registry,
                    ));
                    map
                },
                visible: None,
                inaccessible: false,
                tags: Vec::new(),
                rust_typename: Some(std::any::type_name::<Self>()),
            },
        )
    }

    async fn resolve(
        &self,
        ctx: &ContextSelectionSet<'_>,
        field: &Positioned<Field>,
    ) -> ServerResult<Value> {
        match self {
            Self::IntCounter(v) => v.resolve(ctx, field).await,
            Self::Counter(v) => v.resolve(ctx, field).await,
            Self::IntGauge(v) => v.resolve(ctx, field).await,
            Self::Gauge(v) => v.resolve(ctx, field).await,
            Self::Histogram(v) => v.resolve(ctx, field).await,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenericGauge<T: Atomic> {
    val: core::GenericGauge<T>,
    opts: Opts,
}

impl<T: Atomic> Deref for GenericGauge<T> {
    type Target = core::GenericGauge<T>;

    fn deref(&self) -> &Self::Target {
        &self.val
    }
}

impl<T: Atomic> DerefMut for GenericGauge<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.val
    }
}

impl<T: Atomic> GenericGauge<T> {
    pub fn new<S1: Into<String>, S2: Into<String>>(
        name: S1,
        help: S2,
    ) -> Result<Self, prometheus::Error> {
        let opts = Opts::new(name, help);
        core::GenericGauge::<T>::with_opts(opts.clone()).map(|val| Self { val, opts })
    }

    pub fn with_opts(opts: Opts) -> Result<Self, prometheus::Error> {
        core::GenericGauge::<T>::with_opts(opts.clone()).map(|val| Self { val, opts })
    }
}

#[cfg(feature = "async-graphql")]
#[async_trait::async_trait]
impl<T> async_graphql::OutputType for GenericGauge<T>
where
    T: Atomic,
    <T as Atomic>::T: async_graphql::OutputType,
{
    fn type_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("GenericGauge")
    }

    fn create_type_info(registry: &mut async_graphql::registry::Registry) -> String {
        registry.create_output_type::<Self, _>(async_graphql::registry::MetaTypeId::Scalar, |_| {
            async_graphql::registry::MetaType::Scalar {
                name: Self::type_name().to_string(),
                description: None,
                is_valid: None,
                visible: None,
                inaccessible: false,
                tags: Vec::new(),
                specified_by_url: None,
            }
        })
    }

    async fn resolve(
        &self,
        ctx: &ContextSelectionSet<'_>,
        field: &Positioned<Field>,
    ) -> ServerResult<Value> {
        <<T as Atomic>::T as async_graphql::OutputType>::resolve(&self.val.get(), ctx, field).await
    }
}

impl<T: Atomic> Serialize for GenericGauge<T>
where
    T::T: Serialize + DeserializeOwned,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut ser = serializer.serialize_tuple_struct("GenericCounter", 1)?;
        let opts = SerializableOpts {
            val: self.val.get(),
            opts: self.opts.clone(),
        };
        ser.serialize_field(&opts)?;
        ser.end()
    }
}

impl<'de, T: Atomic> Deserialize<'de> for GenericGauge<T>
where
    T::T: Serialize + DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        <SerializableOpts<T::T> as Deserialize<'de>>::deserialize(deserializer).map(|opt| {
            // unwrap safe here, because ctr is always valid
            let val = prometheus::core::GenericGauge::with_opts(opt.opts.clone()).unwrap();
            val.set(opt.val);
            Self {
                val,
                opts: opt.opts,
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct GenericCounter<T: Atomic> {
    ctr: core::GenericCounter<T>,
    opts: Opts,
}

impl<T: Atomic> Deref for GenericCounter<T> {
    type Target = core::GenericCounter<T>;

    fn deref(&self) -> &Self::Target {
        &self.ctr
    }
}

impl<T: Atomic> DerefMut for GenericCounter<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.ctr
    }
}

impl<T: Atomic> GenericCounter<T> {
    pub fn new<S1: Into<String>, S2: Into<String>>(
        name: S1,
        help: S2,
    ) -> Result<Self, prometheus::Error> {
        let opts = Opts::new(name, help);
        core::GenericCounter::<T>::with_opts(opts.clone()).map(|ctr| Self { ctr, opts })
    }

    pub fn with_opts(opts: Opts) -> Result<Self, prometheus::Error> {
        core::GenericCounter::<T>::with_opts(opts.clone()).map(|ctr| Self { ctr, opts })
    }
}

#[cfg(feature = "async-graphql")]
#[async_trait::async_trait]
impl<T> async_graphql::OutputType for GenericCounter<T>
where
    T: Atomic,
    <T as Atomic>::T: async_graphql::OutputType,
{
    fn type_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("GenericCounter")
    }

    fn create_type_info(registry: &mut async_graphql::registry::Registry) -> String {
        registry.create_output_type::<Self, _>(async_graphql::registry::MetaTypeId::Scalar, |_| {
            async_graphql::registry::MetaType::Scalar {
                name: Self::type_name().to_string(),
                description: None,
                is_valid: None,
                visible: None,
                inaccessible: false,
                tags: Vec::new(),
                specified_by_url: None,
            }
        })
    }

    async fn resolve(
        &self,
        ctx: &ContextSelectionSet<'_>,
        field: &Positioned<Field>,
    ) -> ServerResult<Value> {
        <<T as Atomic>::T as async_graphql::OutputType>::resolve(&self.ctr.get(), ctx, field).await
    }
}

impl<T: Atomic> Serialize for GenericCounter<T>
where
    T::T: Serialize + DeserializeOwned,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut ser = serializer.serialize_tuple_struct("GenericCounter", 1)?;
        let opts = SerializableOpts {
            val: self.ctr.get(),
            opts: self.opts.clone(),
        };
        ser.serialize_field(&opts)?;
        ser.end()
    }
}

impl<'de, T: Atomic> Deserialize<'de> for GenericCounter<T>
where
    T::T: Serialize + DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        <SerializableOpts<T::T> as Deserialize<'de>>::deserialize(deserializer).map(|opt| {
            // unwrap safe here, because ctr is always valid
            let ctr = prometheus::core::GenericCounter::with_opts(opt.opts.clone()).unwrap();
            ctr.inc_by(opt.val);
            Self {
                ctr,
                opts: opt.opts,
            }
        })
    }
}

#[derive(Debug, Clone)]
struct SerializableHistogramOpts {
    val: HistogramSample,
    opts: HistogramOpts,
}

impl Serialize for SerializableHistogramOpts {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut ser = serializer.serialize_struct("Opts", 8)?;
        ser.serialize_field("val", &self.val)?;
        ser.serialize_field("namespace", &self.opts.common_opts.namespace)?;
        ser.serialize_field("subsystem", &self.opts.common_opts.subsystem)?;
        ser.serialize_field("name", &self.opts.common_opts.name)?;
        ser.serialize_field("help", &self.opts.common_opts.help)?;
        ser.serialize_field("const_labels", &self.opts.common_opts.const_labels)?;
        ser.serialize_field("variable_labels", &self.opts.common_opts.variable_labels)?;
        ser.serialize_field("buckets", &self.opts.buckets)?;
        ser.end()
    }
}

impl<'de> Deserialize<'de> for SerializableHistogramOpts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Val,
            Namespace,
            Subsystem,
            Name,
            Help,
            ConstLabels,
            VariableLabels,
            Buckets,
        }

        struct SerializableHistogramOptsVisitor;

        impl<'de> Visitor<'de> for SerializableHistogramOptsVisitor {
            type Value = SerializableHistogramOpts;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct SerializableHistogramOpts")
            }

            fn visit_map<V>(self, mut map: V) -> Result<SerializableHistogramOpts, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut val = None;
                let mut namespace = None;
                let mut subsystem = None;
                let mut name = None;
                let mut help = None;
                let mut const_labels = None;
                let mut variable_labels = None;
                let mut buckets = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Val => {
                            if val.is_some() {
                                return Err(serde::de::Error::duplicate_field("ctr"));
                            }
                            val = Some(map.next_value()?);
                        }
                        Field::Namespace => {
                            if namespace.is_some() {
                                return Err(serde::de::Error::duplicate_field("namespace"));
                            }
                            namespace = Some(map.next_value()?);
                        }
                        Field::Subsystem => {
                            if subsystem.is_some() {
                                return Err(serde::de::Error::duplicate_field("subsystem"));
                            }
                            subsystem = Some(map.next_value()?);
                        }
                        Field::Name => {
                            if name.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name = Some(map.next_value()?);
                        }
                        Field::Help => {
                            if help.is_some() {
                                return Err(serde::de::Error::duplicate_field("help"));
                            }
                            help = Some(map.next_value()?);
                        }
                        Field::ConstLabels => {
                            if const_labels.is_some() {
                                return Err(serde::de::Error::duplicate_field("const_labels"));
                            }
                            const_labels = Some(map.next_value()?);
                        }
                        Field::VariableLabels => {
                            if variable_labels.is_some() {
                                return Err(serde::de::Error::duplicate_field("variable_labels"));
                            }
                            variable_labels = Some(map.next_value()?);
                        }
                        Field::Buckets => {
                            if buckets.is_some() {
                                return Err(serde::de::Error::duplicate_field("buckets"));
                            }
                            buckets = Some(map.next_value()?);
                        }
                    }
                }

                let val = val.ok_or_else(|| serde::de::Error::missing_field("val"))?;
                Ok(SerializableHistogramOpts {
                    val,
                    opts: HistogramOpts {
                        buckets: buckets.unwrap_or_default(),
                        common_opts: Opts {
                            namespace: namespace.unwrap_or_default(),
                            subsystem: subsystem.unwrap_or_default(),
                            name: name.unwrap_or_default(),
                            help: help.unwrap_or_default(),
                            const_labels: const_labels.unwrap_or_default(),
                            variable_labels: variable_labels.unwrap_or_default(),
                        },
                    },
                })
            }
        }

        const FIELDS: &[&str] = &[
            "ctr",
            "namespace",
            "subsystem",
            "name",
            "help",
            "const_labels",
            "variable_label",
            "buckets",
        ];
        deserializer.deserialize_struct(
            "SerializableOpts",
            FIELDS,
            SerializableHistogramOptsVisitor,
        )
    }
}

#[derive(Debug, Clone)]
pub struct Histogram {
    val: prometheus::Histogram,
    opts: HistogramOpts,
}

impl Histogram {
    pub fn with_opts(opts: HistogramOpts) -> Result<Self, prometheus::Error> {
        prometheus::Histogram::with_opts(opts.clone()).map(|val| Self { val, opts })
    }
}

impl Deref for Histogram {
    type Target = prometheus::Histogram;

    fn deref(&self) -> &Self::Target {
        &self.val
    }
}

impl DerefMut for Histogram {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.val
    }
}

impl Serialize for Histogram {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut ser = serializer.serialize_tuple_struct("Histogram", 1)?;
        let sample = HistogramSample {
            count: self.val.get_sample_count(),
            sum: self.val.get_sample_sum(),
        };
        let opts = SerializableHistogramOpts {
            val: sample,
            opts: self.opts.clone(),
        };
        ser.serialize_field(&opts)?;
        ser.end()
    }
}

impl<'de> Deserialize<'de> for Histogram {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        // TODO: this implementation is not correct because we cannot set samples for histogram,
        // need to wait prometheus support serde.
        SerializableHistogramOpts::deserialize(deserializer).map(
            |SerializableHistogramOpts { val, opts }| {
                let x = prometheus::Histogram::with_opts(opts.clone()).unwrap();
                Self { val: x, opts }
            },
        )
    }
}

#[cfg(feature = "async-graphql")]
#[derive(async_graphql::SimpleObject, Debug, Clone, Copy, Serialize, Deserialize)]
#[graphql(name = "Histogram")]
struct HistogramSample {
    count: u64,
    sum: f64,
}

#[cfg(feature = "async-graphql")]
#[async_trait::async_trait]
impl async_graphql::OutputType for Histogram {
    fn type_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Histogram")
    }

    fn create_type_info(registry: &mut async_graphql::registry::Registry) -> String {
        <HistogramSample as async_graphql::OutputType>::create_type_info(registry)
    }

    async fn resolve(
        &self,
        ctx: &ContextSelectionSet<'_>,
        field: &Positioned<Field>,
    ) -> ServerResult<Value> {
        let sample = HistogramSample {
            count: self.val.get_sample_count(),
            sum: self.val.get_sample_sum(),
        };

        <HistogramSample as async_graphql::OutputType>::resolve(&sample, ctx, field).await
    }
}

#[derive(Debug, Clone)]
struct SerializableOpts<T> {
    val: T,
    opts: Opts,
}

impl<T: Serialize + DeserializeOwned> Serialize for SerializableOpts<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut ser = serializer.serialize_struct("Opts", 7)?;
        ser.serialize_field("val", &self.val)?;
        ser.serialize_field("namespace", &self.opts.namespace)?;
        ser.serialize_field("subsystem", &self.opts.subsystem)?;
        ser.serialize_field("name", &self.opts.name)?;
        ser.serialize_field("help", &self.opts.help)?;
        ser.serialize_field("const_labels", &self.opts.const_labels)?;
        ser.serialize_field("variable_labels", &self.opts.variable_labels)?;
        ser.end()
    }
}

impl<'de, T: Serialize + DeserializeOwned> Deserialize<'de> for SerializableOpts<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Val,
            Namespace,
            Subsystem,
            Name,
            Help,
            ConstLabels,
            VariableLabels,
        }

        struct SerializableOptsVisitor<T: Serialize + DeserializeOwned>(
            std::marker::PhantomData<T>,
        );

        impl<'de, T: Serialize + DeserializeOwned> Visitor<'de> for SerializableOptsVisitor<T> {
            type Value = SerializableOpts<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct SerializableOpts")
            }

            fn visit_map<V>(self, mut map: V) -> Result<SerializableOpts<T>, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut val = None;
                let mut namespace = None;
                let mut subsystem = None;
                let mut name = None;
                let mut help = None;
                let mut const_labels = None;
                let mut variable_labels = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Val => {
                            if val.is_some() {
                                return Err(serde::de::Error::duplicate_field("ctr"));
                            }
                            val = Some(map.next_value()?);
                        }
                        Field::Namespace => {
                            if namespace.is_some() {
                                return Err(serde::de::Error::duplicate_field("namespace"));
                            }
                            namespace = Some(map.next_value()?);
                        }
                        Field::Subsystem => {
                            if subsystem.is_some() {
                                return Err(serde::de::Error::duplicate_field("subsystem"));
                            }
                            subsystem = Some(map.next_value()?);
                        }
                        Field::Name => {
                            if name.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name = Some(map.next_value()?);
                        }
                        Field::Help => {
                            if help.is_some() {
                                return Err(serde::de::Error::duplicate_field("help"));
                            }
                            help = Some(map.next_value()?);
                        }
                        Field::ConstLabels => {
                            if const_labels.is_some() {
                                return Err(serde::de::Error::duplicate_field("const_labels"));
                            }
                            const_labels = Some(map.next_value()?);
                        }
                        Field::VariableLabels => {
                            if variable_labels.is_some() {
                                return Err(serde::de::Error::duplicate_field("variable_labels"));
                            }
                            variable_labels = Some(map.next_value()?);
                        }
                    }
                }

                let val = val.ok_or_else(|| serde::de::Error::missing_field("val"))?;
                Ok(SerializableOpts {
                    val,
                    opts: Opts {
                        namespace: namespace.unwrap_or_default(),
                        subsystem: subsystem.unwrap_or_default(),
                        name: name.unwrap_or_default(),
                        help: help.unwrap_or_default(),
                        const_labels: const_labels.unwrap_or_default(),
                        variable_labels: variable_labels.unwrap_or_default(),
                    },
                })
            }
        }

        const FIELDS: &[&str] = &[
            "ctr",
            "namespace",
            "subsystem",
            "name",
            "help",
            "const_labels",
            "variable_label",
        ];
        deserializer.deserialize_struct(
            "SerializableOpts",
            FIELDS,
            SerializableOptsVisitor::<T>(std::marker::PhantomData),
        )
    }
}

macro_rules! metric_typ {
    ($($ty: ident::$setter:ident($primitive:ident)::$name: literal), +$(,)?) => {
        $(
            #[derive(Clone)]
            pub struct $ty {
                val: prometheus::$ty,
                opts: Opts,
            }

            impl std::fmt::Debug for $ty {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    <prometheus::$ty as std::fmt::Debug>::fmt(&self.val, f)
                }
            }

            impl Serialize for $ty {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::ser::Serializer,
                {
                    let mut ser = serializer.serialize_tuple_struct($name, 1)?;
                    let opts = SerializableOpts {
                        val: self.val.get(),
                        opts: self.opts.clone(),
                    };
                    ser.serialize_field(&opts)?;
                    ser.end()
                }
            }

            impl<'de> Deserialize<'de> for $ty {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de> {
                    <SerializableOpts<$primitive> as Deserialize<'de>>::deserialize(deserializer).map(|opt| {
                        // unwrap safe here, because ctr is always valid
                        let val = prometheus::$ty::with_opts(opt.opts.clone()).unwrap();
                        val.$setter(opt.val);
                        Self { val, opts: opt.opts }
                    })
                }
            }

            impl $ty {
                pub fn new<S1: Into<String>, S2: Into<String>>(
                    name: S1,
                    help: S2,
                ) -> Result<Self, prometheus::Error> {
                    let opts = Opts::new(name, help);
                    prometheus::$ty::with_opts(opts.clone()).map(|val| Self {
                        val,
                        opts,
                    })
                }

                pub fn with_opts(opts: Opts) -> Result<Self, prometheus::Error> {
                    prometheus::$ty::with_opts(opts.clone()).map(|val| Self {
                        val,
                        opts,
                    })
                }
            }

            impl Deref for $ty {
                type Target = prometheus::$ty;

                fn deref(&self) -> &Self::Target {
                    &self.val
                }
            }

            impl DerefMut for $ty {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    &mut self.val
                }
            }

            #[cfg(feature = "async-graphql")]
            #[async_trait::async_trait]
            impl async_graphql::OutputType for $ty {
                fn type_name() -> std::borrow::Cow<'static, str> {
                    std::borrow::Cow::Borrowed($name)
                }

                fn create_type_info(registry: &mut async_graphql::registry::Registry) -> String {
                    registry.create_output_type::<Self, _>(async_graphql::registry::MetaTypeId::Scalar, |_| {
                        async_graphql::registry::MetaType::Scalar {
                            name: Self::type_name().to_string(),
                            description: None,
                            is_valid: None,
                            visible: None,
                            inaccessible: false,
                            tags: Vec::new(),
                            specified_by_url: None,
                        }
                    })
                }

                async fn resolve(
                    &self,
                    ctx: &ContextSelectionSet<'_>,
                    field: &Positioned<Field>,
                ) -> ServerResult<Value> {
                    <$primitive as async_graphql::OutputType>::resolve(&self.val.get(), ctx, field).await
                }
            }
        )*
    };
}

metric_typ! {
    IntCounter::inc_by(u64)::"IntCounter",
    Counter::inc_by(f64)::"Counter",
    IntGauge::set(i64)::"IntGauge",
    Gauge::set(f64)::"Gauge",
}
