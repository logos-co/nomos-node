#![feature(prelude_import)]
#[prelude_import]
use std::prelude::rust_2021::*;
#[macro_use]
extern crate std;
use std::{collections::HashSet, path::{Path, PathBuf}};
use nomos_core::{header::HeaderId, tx::mock::{MockTransaction, MockTxId}};
use nomos_mempool::{
    backend::mockpool::MockPool,
    network::adapters::mock::{MockAdapter, MOCK_TX_CONTENT_TOPIC},
    tx::{service::GenericTxMempoolService, state::TxMempoolState},
    MempoolMsg, TxMempoolSettings,
};
use nomos_network::{
    backends::mock::{Mock, MockBackendMessage, MockConfig, MockMessage},
    NetworkConfig, NetworkMsg, NetworkService,
};
use nomos_tracing_service::{Tracing, TracingSettings};
use overwatch::overwatch::OverwatchRunner;
use overwatch_derive::*;
use rand::distributions::{Alphanumeric, DistString};
use services_utils::{
    overwatch::{recovery::operators::RecoveryBackend, JsonFileBackend},
    traits::FromSettings,
};
type MockRecoveryBackend = JsonFileBackend<
    TxMempoolState<MockPool<HeaderId, MockTransaction<MockMessage>, MockTxId>, (), ()>,
    TxMempoolSettings<(), ()>,
>;
type MockMempoolService<RuntimeServiceId> = GenericTxMempoolService<
    MockPool<HeaderId, MockTransaction<MockMessage>, MockTxId>,
    MockAdapter<RuntimeServiceId>,
    MockRecoveryBackend,
    RuntimeServiceId,
>;
struct MockPoolNode {
    logging: ::overwatch::OpaqueServiceHandle<
        Tracing<RuntimeServiceId>,
        RuntimeServiceId,
    >,
    network: ::overwatch::OpaqueServiceHandle<
        NetworkService<Mock, RuntimeServiceId>,
        RuntimeServiceId,
    >,
    mockpool: ::overwatch::OpaqueServiceHandle<
        MockMempoolService<RuntimeServiceId>,
        RuntimeServiceId,
    >,
}
pub enum RuntimeServiceId {
    Logging,
    Network,
    Mockpool,
}
#[automatically_derived]
impl ::core::fmt::Debug for RuntimeServiceId {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        ::core::fmt::Formatter::write_str(
            f,
            match self {
                RuntimeServiceId::Logging => "Logging",
                RuntimeServiceId::Network => "Network",
                RuntimeServiceId::Mockpool => "Mockpool",
            },
        )
    }
}
#[automatically_derived]
impl ::core::clone::Clone for RuntimeServiceId {
    #[inline]
    fn clone(&self) -> RuntimeServiceId {
        *self
    }
}
#[automatically_derived]
impl ::core::marker::Copy for RuntimeServiceId {}
#[automatically_derived]
impl ::core::marker::StructuralPartialEq for RuntimeServiceId {}
#[automatically_derived]
impl ::core::cmp::PartialEq for RuntimeServiceId {
    #[inline]
    fn eq(&self, other: &RuntimeServiceId) -> bool {
        let __self_discr = ::core::intrinsics::discriminant_value(self);
        let __arg1_discr = ::core::intrinsics::discriminant_value(other);
        __self_discr == __arg1_discr
    }
}
#[automatically_derived]
impl ::core::cmp::Eq for RuntimeServiceId {
    #[inline]
    #[doc(hidden)]
    #[coverage(off)]
    fn assert_receiver_is_total_eq(&self) -> () {}
}
pub struct RuntimeLifeCycleHandlers {
    logging: ::overwatch::services::life_cycle::LifecycleHandle,
    network: ::overwatch::services::life_cycle::LifecycleHandle,
    mockpool: ::overwatch::services::life_cycle::LifecycleHandle,
}
impl ::overwatch::overwatch::life_cycle::ServicesLifeCycleHandle<RuntimeServiceId>
for RuntimeLifeCycleHandlers {
    type Error = ::overwatch::DynError;
    /// Send a `Shutdown` message to the specified service.
    ///
    /// # Arguments
    ///
    /// `service` - The [`ServiceId`] of the target service
    /// `sender` - The sender side of a broadcast channel. It's expected that
    /// once the receiver finishes processing the message, a signal will be
    /// sent back.
    ///
    /// # Errors
    ///
    /// The error returned when trying to send the shutdown command to the
    /// specified service.
    fn shutdown(
        &self,
        service: &RuntimeServiceId,
        sender: ::tokio::sync::broadcast::Sender<
            ::overwatch::services::life_cycle::FinishedSignal,
        >,
    ) -> Result<(), Self::Error> {
        match service {
            &RuntimeServiceId::Logging => {
                self.logging
                    .send(
                        ::overwatch::services::life_cycle::LifecycleMessage::Shutdown(
                            sender,
                        ),
                    )
            }
            &RuntimeServiceId::Network => {
                self.network
                    .send(
                        ::overwatch::services::life_cycle::LifecycleMessage::Shutdown(
                            sender,
                        ),
                    )
            }
            &RuntimeServiceId::Mockpool => {
                self.mockpool
                    .send(
                        ::overwatch::services::life_cycle::LifecycleMessage::Shutdown(
                            sender,
                        ),
                    )
            }
        }
    }
    /// Send a [`LifecycleMessage::Kill`] message to the specified service
    /// ([`ServiceId`]) [`crate::overwatch::OverwatchRunner`].
    /// # Arguments
    ///
    /// `service` - The [`ServiceId`] of the target service
    ///
    /// # Errors
    ///
    /// The error returned when trying to send the kill command to the specified
    /// service.
    fn kill(&self, service: &RuntimeServiceId) -> Result<(), Self::Error> {
        match service {
            &RuntimeServiceId::Logging => {
                self.logging
                    .send(::overwatch::services::life_cycle::LifecycleMessage::Kill)
            }
            &RuntimeServiceId::Network => {
                self.network
                    .send(::overwatch::services::life_cycle::LifecycleMessage::Kill)
            }
            &RuntimeServiceId::Mockpool => {
                self.mockpool
                    .send(::overwatch::services::life_cycle::LifecycleMessage::Kill)
            }
        }
    }
    /// Send a [`LifecycleMessage::Kill`] message to all services registered in
    /// this handle.
    ///
    /// # Errors
    ///
    /// The error returned when trying to send the kill command to any of the
    /// running services.
    fn kill_all(&self) -> Result<(), Self::Error> {
        self.logging.send(::overwatch::services::life_cycle::LifecycleMessage::Kill)?;
        self.network.send(::overwatch::services::life_cycle::LifecycleMessage::Kill)?;
        self.mockpool.send(::overwatch::services::life_cycle::LifecycleMessage::Kill)?;
        Ok(())
    }
}
impl ::core::fmt::Display for RuntimeServiceId {
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        <Self as ::core::fmt::Debug>::fmt(self, f)
    }
}
impl ::overwatch::services::ToService<Tracing<RuntimeServiceId>> for RuntimeServiceId {
    const SERVICE: Self = RuntimeServiceId::Logging;
}
impl ::overwatch::services::ToService<NetworkService<Mock, RuntimeServiceId>>
for RuntimeServiceId {
    const SERVICE: Self = RuntimeServiceId::Network;
}
impl ::overwatch::services::ToService<MockMempoolService<RuntimeServiceId>>
for RuntimeServiceId {
    const SERVICE: Self = RuntimeServiceId::Mockpool;
}
pub struct MockPoolNodeServiceSettings {
    pub logging: <Tracing<
        RuntimeServiceId,
    > as ::overwatch::services::ServiceData>::Settings,
    pub network: <NetworkService<
        Mock,
        RuntimeServiceId,
    > as ::overwatch::services::ServiceData>::Settings,
    pub mockpool: <MockMempoolService<
        RuntimeServiceId,
    > as ::overwatch::services::ServiceData>::Settings,
}
#[automatically_derived]
impl ::core::clone::Clone for MockPoolNodeServiceSettings {
    #[inline]
    fn clone(&self) -> MockPoolNodeServiceSettings {
        MockPoolNodeServiceSettings {
            logging: ::core::clone::Clone::clone(&self.logging),
            network: ::core::clone::Clone::clone(&self.network),
            mockpool: ::core::clone::Clone::clone(&self.mockpool),
        }
    }
}
#[automatically_derived]
impl ::core::fmt::Debug for MockPoolNodeServiceSettings {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        ::core::fmt::Formatter::debug_struct_field3_finish(
            f,
            "MockPoolNodeServiceSettings",
            "logging",
            &self.logging,
            "network",
            &self.network,
            "mockpool",
            &&self.mockpool,
        )
    }
}
impl ::overwatch::overwatch::Services for MockPoolNode {
    type Settings = MockPoolNodeServiceSettings;
    type RuntimeServiceId = RuntimeServiceId;
    type ServicesLifeCycleHandle = RuntimeLifeCycleHandlers;
    fn new(
        settings: Self::Settings,
        overwatch_handle: ::overwatch::overwatch::handle::OverwatchHandle<
            Self::RuntimeServiceId,
        >,
    ) -> ::core::result::Result<Self, ::overwatch::DynError> {
        let Self::Settings {
            logging: logging_settings,
            network: network_settings,
            mockpool: mockpool_settings,
        } = settings;
        let app = Self {
            logging: {
                let manager = ::overwatch::OpaqueServiceHandle::<
                    Tracing<RuntimeServiceId>,
                    Self::RuntimeServiceId,
                >::new::<
                    <Tracing<
                        RuntimeServiceId,
                    > as ::overwatch::services::ServiceData>::StateOperator,
                >(
                    logging_settings,
                    overwatch_handle.clone(),
                    <Tracing<
                        RuntimeServiceId,
                    > as ::overwatch::services::ServiceData>::SERVICE_RELAY_BUFFER_SIZE,
                )?;
                manager
            },
            network: {
                let manager = ::overwatch::OpaqueServiceHandle::<
                    NetworkService<Mock, RuntimeServiceId>,
                    Self::RuntimeServiceId,
                >::new::<
                    <NetworkService<
                        Mock,
                        RuntimeServiceId,
                    > as ::overwatch::services::ServiceData>::StateOperator,
                >(
                    network_settings,
                    overwatch_handle.clone(),
                    <NetworkService<
                        Mock,
                        RuntimeServiceId,
                    > as ::overwatch::services::ServiceData>::SERVICE_RELAY_BUFFER_SIZE,
                )?;
                manager
            },
            mockpool: {
                let manager = ::overwatch::OpaqueServiceHandle::<
                    MockMempoolService<RuntimeServiceId>,
                    Self::RuntimeServiceId,
                >::new::<
                    <MockMempoolService<
                        RuntimeServiceId,
                    > as ::overwatch::services::ServiceData>::StateOperator,
                >(
                    mockpool_settings,
                    overwatch_handle.clone(),
                    <MockMempoolService<
                        RuntimeServiceId,
                    > as ::overwatch::services::ServiceData>::SERVICE_RELAY_BUFFER_SIZE,
                )?;
                manager
            },
        };
        ::core::result::Result::Ok(app)
    }
    fn start_all(
        &mut self,
    ) -> ::core::result::Result<
        Self::ServicesLifeCycleHandle,
        ::overwatch::overwatch::Error,
    > {
        ::core::result::Result::Ok(Self::ServicesLifeCycleHandle {
            logging: self
                .logging
                .service_runner::<
                    <Tracing<
                        RuntimeServiceId,
                    > as ::overwatch::services::ServiceData>::StateOperator,
                >()
                .run::<Tracing<RuntimeServiceId>>()?,
            network: self
                .network
                .service_runner::<
                    <NetworkService<
                        Mock,
                        RuntimeServiceId,
                    > as ::overwatch::services::ServiceData>::StateOperator,
                >()
                .run::<NetworkService<Mock, RuntimeServiceId>>()?,
            mockpool: self
                .mockpool
                .service_runner::<
                    <MockMempoolService<
                        RuntimeServiceId,
                    > as ::overwatch::services::ServiceData>::StateOperator,
                >()
                .run::<MockMempoolService<RuntimeServiceId>>()?,
        })
    }
    fn start(
        &mut self,
        service_id: &Self::RuntimeServiceId,
    ) -> ::core::result::Result<(), ::overwatch::overwatch::Error> {
        match service_id {
            &<Tracing<
                RuntimeServiceId,
            > as ::overwatch::services::ServiceId<RuntimeServiceId>>::SERVICE_ID => {
                self.logging
                    .service_runner::<
                        <Tracing<
                            RuntimeServiceId,
                        > as ::overwatch::services::ServiceData>::StateOperator,
                    >()
                    .run::<Tracing<RuntimeServiceId>>()?;
                ::core::result::Result::Ok(())
            }
            &<NetworkService<
                Mock,
                RuntimeServiceId,
            > as ::overwatch::services::ServiceId<RuntimeServiceId>>::SERVICE_ID => {
                self.network
                    .service_runner::<
                        <NetworkService<
                            Mock,
                            RuntimeServiceId,
                        > as ::overwatch::services::ServiceData>::StateOperator,
                    >()
                    .run::<NetworkService<Mock, RuntimeServiceId>>()?;
                ::core::result::Result::Ok(())
            }
            &<MockMempoolService<
                RuntimeServiceId,
            > as ::overwatch::services::ServiceId<RuntimeServiceId>>::SERVICE_ID => {
                self.mockpool
                    .service_runner::<
                        <MockMempoolService<
                            RuntimeServiceId,
                        > as ::overwatch::services::ServiceData>::StateOperator,
                    >()
                    .run::<MockMempoolService<RuntimeServiceId>>()?;
                ::core::result::Result::Ok(())
            }
        }
    }
    fn stop(&mut self, service_id: &Self::RuntimeServiceId) {
        match service_id {
            &<Tracing<
                RuntimeServiceId,
            > as ::overwatch::services::ServiceId<RuntimeServiceId>>::SERVICE_ID => {
                ::core::panicking::panic("not implemented")
            }
            &<NetworkService<
                Mock,
                RuntimeServiceId,
            > as ::overwatch::services::ServiceId<RuntimeServiceId>>::SERVICE_ID => {
                ::core::panicking::panic("not implemented")
            }
            &<MockMempoolService<
                RuntimeServiceId,
            > as ::overwatch::services::ServiceId<RuntimeServiceId>>::SERVICE_ID => {
                ::core::panicking::panic("not implemented")
            }
        }
    }
    fn request_relay(
        &mut self,
        service_id: &Self::RuntimeServiceId,
    ) -> ::overwatch::services::relay::RelayResult {
        match service_id {
            &<Tracing<
                RuntimeServiceId,
            > as ::overwatch::services::ServiceId<RuntimeServiceId>>::SERVICE_ID => {
                ::core::result::Result::Ok(
                    ::std::boxed::Box::new(
                        self
                            .logging
                            .relay_with()
                            .ok_or(
                                ::overwatch::services::relay::RelayError::AlreadyConnected,
                            )?,
                    ) as ::overwatch::services::relay::AnyMessage,
                )
            }
            &<NetworkService<
                Mock,
                RuntimeServiceId,
            > as ::overwatch::services::ServiceId<RuntimeServiceId>>::SERVICE_ID => {
                ::core::result::Result::Ok(
                    ::std::boxed::Box::new(
                        self
                            .network
                            .relay_with()
                            .ok_or(
                                ::overwatch::services::relay::RelayError::AlreadyConnected,
                            )?,
                    ) as ::overwatch::services::relay::AnyMessage,
                )
            }
            &<MockMempoolService<
                RuntimeServiceId,
            > as ::overwatch::services::ServiceId<RuntimeServiceId>>::SERVICE_ID => {
                ::core::result::Result::Ok(
                    ::std::boxed::Box::new(
                        self
                            .mockpool
                            .relay_with()
                            .ok_or(
                                ::overwatch::services::relay::RelayError::AlreadyConnected,
                            )?,
                    ) as ::overwatch::services::relay::AnyMessage,
                )
            }
        }
    }
    fn request_status_watcher(
        &self,
        service_id: &Self::RuntimeServiceId,
    ) -> ::overwatch::services::status::StatusWatcher {
        match service_id {
            &<Tracing<
                RuntimeServiceId,
            > as ::overwatch::services::ServiceId<RuntimeServiceId>>::SERVICE_ID => {
                self.logging.status_watcher()
            }
            &<NetworkService<
                Mock,
                RuntimeServiceId,
            > as ::overwatch::services::ServiceId<RuntimeServiceId>>::SERVICE_ID => {
                self.network.status_watcher()
            }
            &<MockMempoolService<
                RuntimeServiceId,
            > as ::overwatch::services::ServiceId<RuntimeServiceId>>::SERVICE_ID => {
                self.mockpool.status_watcher()
            }
        }
    }
    fn update_settings(&mut self, settings: Self::Settings) {
        let Self::Settings {
            logging: logging_settings,
            network: network_settings,
            mockpool: mockpool_settings,
        } = settings;
        self.logging.update_settings(logging_settings);
        self.network.update_settings(network_settings);
        self.mockpool.update_settings(mockpool_settings);
    }
}
fn run_with_recovery_teardown(recovery_path: &Path, run: impl Fn()) {
    run();
    let _ = std::fs::remove_file(recovery_path);
}
fn get_test_random_path() -> PathBuf {
    PathBuf::from(Alphanumeric.sample_string(&mut rand::thread_rng(), 5))
        .with_extension(".json")
}
extern crate test;
#[cfg(test)]
#[rustc_test_marker = "test_mockmempool"]
#[doc(hidden)]
pub const test_mockmempool: test::TestDescAndFn = test::TestDescAndFn {
    desc: test::TestDesc {
        name: test::StaticTestName("test_mockmempool"),
        ignore: false,
        ignore_message: ::core::option::Option::None,
        source_file: "nomos-services/mempool/tests/mock.rs",
        start_line: 57usize,
        start_col: 4usize,
        end_line: 57usize,
        end_col: 20usize,
        compile_fail: false,
        no_run: false,
        should_panic: test::ShouldPanic::No,
        test_type: test::TestType::IntegrationTest,
    },
    testfn: test::StaticTestFn(
        #[coverage(off)]
        || test::assert_test_result(test_mockmempool()),
    ),
};
fn test_mockmempool() {
    let recovery_file_path = get_test_random_path();
    run_with_recovery_teardown(
        &recovery_file_path,
        || {
            let exist = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let exist2 = exist.clone();
            let predefined_messages = <[_]>::into_vec(
                #[rustc_box]
                ::alloc::boxed::Box::new([
                    MockMessage {
                        payload: "This is foo".to_owned(),
                        content_topic: MOCK_TX_CONTENT_TOPIC,
                        version: 0,
                        timestamp: 0,
                    },
                    MockMessage {
                        payload: "This is bar".to_owned(),
                        content_topic: MOCK_TX_CONTENT_TOPIC,
                        version: 0,
                        timestamp: 0,
                    },
                ]),
            );
            let exp_txns: HashSet<MockMessage> = predefined_messages
                .iter()
                .cloned()
                .collect();
            let app = OverwatchRunner::<
                MockPoolNode,
            >::run(
                    MockPoolNodeServiceSettings {
                        network: NetworkConfig {
                            backend: MockConfig {
                                predefined_messages,
                                duration: tokio::time::Duration::from_millis(100),
                                seed: 0,
                                version: 1,
                                weights: None,
                            },
                        },
                        mockpool: TxMempoolSettings {
                            pool: (),
                            network_adapter: (),
                            recovery_path: recovery_file_path.clone(),
                        },
                        logging: TracingSettings::default(),
                    },
                    None,
                )
                .map_err(|e| {
                    ::std::io::_eprint(format_args!("Error encountered: {0}\n", e));
                })
                .unwrap();
            let overwatch_handle = app.handle().clone();
            app.spawn(async move {
                let network_outbound = overwatch_handle
                    .relay::<NetworkService<Mock, RuntimeServiceId>>()
                    .await
                    .unwrap();
                let mempool_outbound = overwatch_handle
                    .relay::<MockMempoolService<RuntimeServiceId>>()
                    .await
                    .unwrap();
                network_outbound
                    .send(
                        NetworkMsg::Process(MockBackendMessage::RelaySubscribe {
                            topic: MOCK_TX_CONTENT_TOPIC.content_topic_name.to_string(),
                        }),
                    )
                    .await
                    .unwrap();
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    let (mtx, mrx) = tokio::sync::oneshot::channel();
                    mempool_outbound
                        .send(MempoolMsg::View {
                            ancestor_hint: [0; 32].into(),
                            reply_channel: mtx,
                        })
                        .await
                        .unwrap();
                    let items: HashSet<MockMessage> = mrx
                        .await
                        .unwrap()
                        .map(|msg| msg.message().clone())
                        .collect();
                    if items.len() == exp_txns.len() {
                        match (&exp_txns, &items) {
                            (left_val, right_val) => {
                                if !(*left_val == *right_val) {
                                    let kind = ::core::panicking::AssertKind::Eq;
                                    ::core::panicking::assert_failed(
                                        kind,
                                        &*left_val,
                                        &*right_val,
                                        ::core::option::Option::None,
                                    );
                                }
                            }
                        };
                        exist.store(true, std::sync::atomic::Ordering::SeqCst);
                        break;
                    }
                }
            });
            while !exist2.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            let recovery_backend = MockRecoveryBackend::from_settings(
                &TxMempoolSettings {
                    pool: (),
                    network_adapter: (),
                    recovery_path: recovery_file_path.clone(),
                },
            );
            let recovered_state = recovery_backend
                .load_state()
                .expect("Should not fail to load the state.");
            match (&recovered_state.pool().unwrap().pending_items().len(), &2) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            match (&recovered_state.pool().unwrap().in_block_items().len(), &0) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            };
            if !(recovered_state.pool().unwrap().last_item_timestamp() > 0) {
                ::core::panicking::panic(
                    "assertion failed: recovered_state.pool().unwrap().last_item_timestamp() > 0",
                )
            }
        },
    );
}
#[rustc_main]
#[coverage(off)]
#[doc(hidden)]
pub fn main() -> () {
    extern crate test;
    test::test_main_static(&[&test_mockmempool])
}
