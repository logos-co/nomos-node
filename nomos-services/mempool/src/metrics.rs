use std::fmt::Debug;

use nomos_metrics::prometheus_client::{
    self, encoding::EncodeLabelSet, encoding::EncodeLabelValue,
};
use nomos_metrics::NomosRegistry;

use nomos_metrics::metrics::{counter::Counter, family::Family};
use overwatch_rs::services::ServiceId;

use crate::MempoolMsg;

#[derive(Debug, Clone, Hash, PartialEq, Eq, EncodeLabelValue)]
enum MempoolMsgType {
    Add,
    View,
    Prune,
    MarkInBlock,
}

impl<I, K> From<&MempoolMsg<I, K>> for MempoolMsgType
where
    I: 'static + Debug,
    K: 'static + Debug,
{
    fn from(event: &MempoolMsg<I, K>) -> Self {
        match event {
            MempoolMsg::Add { .. } => MempoolMsgType::Add,
            MempoolMsg::View { .. } => MempoolMsgType::View,
            MempoolMsg::Prune { .. } => MempoolMsgType::Prune,
            MempoolMsg::MarkInBlock { .. } => MempoolMsgType::MarkInBlock,
            _ => unimplemented!(),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, EncodeLabelSet)]
struct MessageLabels {
    label: MempoolMsgType,
}

pub(crate) struct Metrics {
    messages: Family<MessageLabels, Counter>,
}

impl Metrics {
    pub(crate) fn new(registry: NomosRegistry, discriminant: ServiceId) -> Self {
        let mut registry = registry
            .lock()
            .expect("should've acquired the lock for registry");
        let sub_registry = registry.sub_registry_with_prefix(discriminant);

        let messages = Family::default();
        sub_registry.register(
            "messages",
            "Messages emitted by the relay Mempool",
            messages.clone(),
        );

        Self { messages }
    }

    pub(crate) fn record<I, K>(&self, msg: &MempoolMsg<I, K>)
    where
        I: 'static + Debug,
        K: 'static + Debug,
    {
        match msg {
            MempoolMsg::Add { .. }
            | MempoolMsg::View { .. }
            | MempoolMsg::Prune { .. }
            | MempoolMsg::MarkInBlock { .. } => {
                self.messages
                    .get_or_create(&MessageLabels { label: msg.into() })
                    .inc();
            }
            _ => {}
        }
    }
}
