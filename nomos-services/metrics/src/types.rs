use ::core::ops::{Deref, DerefMut};
#[cfg(feature = "async-graphql")]
use async_graphql::{parser::types::Field, ContextSelectionSet, Positioned, ServerResult, Value};
use prometheus::HistogramOpts;
pub use prometheus::{
    core::{self, Atomic},
    labels, opts, Opts,
};

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
pub struct GenericGauge<T: Atomic>(core::GenericGauge<T>);

impl<T: Atomic> Deref for GenericGauge<T> {
    type Target = core::GenericGauge<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Atomic> DerefMut for GenericGauge<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: Atomic> GenericGauge<T> {
    pub fn new<S1: Into<String>, S2: Into<String>>(
        name: S1,
        help: S2,
    ) -> Result<Self, prometheus::Error> {
        let opts = Opts::new(name, help);
        core::GenericGauge::<T>::with_opts(opts).map(Self)
    }

    pub fn with_opts(opts: Opts) -> Result<Self, prometheus::Error> {
        core::GenericGauge::<T>::with_opts(opts).map(Self)
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
        <<T as Atomic>::T as async_graphql::OutputType>::resolve(&self.0.get(), ctx, field).await
    }
}

#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct Gauge(prometheus::Gauge);

impl Deref for Gauge {
    type Target = prometheus::Gauge;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Gauge {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Gauge {
    pub fn new<S1: Into<String>, S2: Into<String>>(
        name: S1,
        help: S2,
    ) -> Result<Self, prometheus::Error> {
        prometheus::Gauge::new(name, help).map(Self)
    }

    pub fn with_opts(opts: Opts) -> Result<Self, prometheus::Error> {
        prometheus::Gauge::with_opts(opts).map(Self)
    }
}

#[cfg(feature = "async-graphql")]
#[async_trait::async_trait]
impl async_graphql::OutputType for Gauge {
    fn type_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Gauge")
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
        <f64 as async_graphql::OutputType>::resolve(&self.0.get(), ctx, field).await
    }
}

#[derive(Debug, Clone)]
pub struct GenericCounter<T: Atomic>(core::GenericCounter<T>);

impl<T: Atomic> Deref for GenericCounter<T> {
    type Target = core::GenericCounter<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Atomic> DerefMut for GenericCounter<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: Atomic> GenericCounter<T> {
    pub fn new<S1: Into<String>, S2: Into<String>>(
        name: S1,
        help: S2,
    ) -> Result<Self, prometheus::Error> {
        core::GenericCounter::<T>::new(name, help).map(Self)
    }

    pub fn with_opts(opts: Opts) -> Result<Self, prometheus::Error> {
        core::GenericCounter::<T>::with_opts(opts).map(Self)
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
        <<T as Atomic>::T as async_graphql::OutputType>::resolve(&self.0.get(), ctx, field).await
    }
}

#[derive(Debug, Clone)]
pub struct Counter(prometheus::Counter);

impl Deref for Counter {
    type Target = prometheus::Counter;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Counter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Counter {
    pub fn new<S1: Into<String>, S2: Into<String>>(
        name: S1,
        help: S2,
    ) -> Result<Self, prometheus::Error> {
        prometheus::Counter::new(name, help).map(Self)
    }

    pub fn with_opts(opts: Opts) -> Result<Self, prometheus::Error> {
        prometheus::Counter::with_opts(opts).map(Self)
    }
}

#[cfg(feature = "async-graphql")]
#[async_trait::async_trait]
impl async_graphql::OutputType for Counter {
    fn type_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Counter")
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
        <f64 as async_graphql::OutputType>::resolve(&self.0.get(), ctx, field).await
    }
}

#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct Histogram(prometheus::Histogram);

impl Histogram {
    pub fn with_opts(opts: HistogramOpts) -> Result<Self, prometheus::Error> {
        prometheus::Histogram::with_opts(opts).map(Self)
    }
}

impl Deref for Histogram {
    type Target = prometheus::Histogram;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Histogram {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(feature = "async-graphql")]
#[derive(async_graphql::SimpleObject)]
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
            count: self.0.get_sample_count(),
            sum: self.0.get_sample_sum(),
        };

        <HistogramSample as async_graphql::OutputType>::resolve(&sample, ctx, field).await
    }
}

#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct IntCounter(prometheus::IntCounter);

impl IntCounter {
    pub fn new<S1: Into<String>, S2: Into<String>>(
        name: S1,
        help: S2,
    ) -> Result<Self, prometheus::Error> {
        prometheus::IntCounter::new(name, help).map(Self)
    }

    pub fn with_opts(opts: Opts) -> Result<Self, prometheus::Error> {
        prometheus::IntCounter::with_opts(opts).map(Self)
    }
}

impl Deref for IntCounter {
    type Target = prometheus::IntCounter;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for IntCounter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(feature = "async-graphql")]
#[async_trait::async_trait]
impl async_graphql::OutputType for IntCounter {
    fn type_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("IntCounter")
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
        <u64 as async_graphql::OutputType>::resolve(&self.0.get(), ctx, field).await
    }
}

#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct IntGauge(prometheus::IntGauge);

impl Deref for IntGauge {
    type Target = prometheus::IntGauge;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for IntGauge {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntGauge {
    pub fn new<S1: Into<String>, S2: Into<String>>(
        name: S1,
        help: S2,
    ) -> Result<Self, prometheus::Error> {
        prometheus::IntGauge::new(name, help).map(Self)
    }

    pub fn with_opts(opts: Opts) -> Result<Self, prometheus::Error> {
        prometheus::IntGauge::with_opts(opts).map(Self)
    }
}

#[cfg(feature = "async-graphql")]
#[async_trait::async_trait]
impl async_graphql::OutputType for IntGauge {
    fn type_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("IntGauge")
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
        <i64 as async_graphql::OutputType>::resolve(&self.0.get(), ctx, field).await
    }
}
