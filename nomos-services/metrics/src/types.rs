use ::core::ops::{Deref, DerefMut};
#[cfg(feature = "async-graphql")]
use async_graphql::{parser::types::Field, ContextSelectionSet, Positioned, ServerResult, Value};
use prometheus::HistogramOpts;
pub use prometheus::{
    core::{self, Atomic, GenericCounter as PrometheusGenericCounter},
    labels, opts, Opts,
};

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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

#[cfg(feature = "async-graphql")]
#[derive(async_graphql::SimpleObject, Debug, Clone, Copy)]
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
