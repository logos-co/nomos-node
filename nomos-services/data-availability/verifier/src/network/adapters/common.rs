macro_rules! adapter_for {
    ($DaNetworkBackend:ident, $DaNetworksEventKind:ident, $DaNetworkEvent:ident) => {
        pub struct Libp2pAdapter<M>
        where
            M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
                + Clone
                + Debug
                + Send
                + Sync
                + 'static,
        {
            network_relay:
                OutboundRelay<<NetworkService<$DaNetworkBackend<M>> as ServiceData>::Message>,
            _membership: PhantomData<M>,
        }

        #[async_trait::async_trait]
        impl<M> NetworkAdapter for Libp2pAdapter<M>
        where
            M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
                + Clone
                + Debug
                + Send
                + Sync
                + 'static,
        {
            type Backend = $DaNetworkBackend<M>;
            type Settings = ();
            type Blob = DaBlob;

            async fn new(
                _settings: Self::Settings,
                network_relay: OutboundRelay<
                    <NetworkService<Self::Backend> as ServiceData>::Message,
                >,
            ) -> Self {
                Self {
                    network_relay,
                    _membership: Default::default(),
                }
            }

            async fn blob_stream(&self) -> Box<dyn Stream<Item = Self::Blob> + Unpin + Send> {
                let (sender, receiver) = tokio::sync::oneshot::channel();
                self.network_relay
                    .send(nomos_da_network_service::DaNetworkMsg::Subscribe {
                        kind: $DaNetworksEventKind::Verifying,
                        sender,
                    })
                    .await
                    .expect("Network backend should be ready");

                let receiver = receiver.await.expect("Blob stream should be received");

                let stream = receiver.filter_map(move |msg| match msg {
                    $DaNetworkEvent::Verifying(blob) => Some(*blob),
                    _ => None,
                });

                Box::new(Box::pin(stream))
            }
        }
    };
}

pub(crate) use adapter_for;
