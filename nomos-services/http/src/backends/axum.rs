// std
use std::{collections::HashMap, pin::Pin, sync::Arc};

// crates
use axum::{
    extract::{Extension, Path},
    response::IntoResponse,
    routing::{delete, get, patch, post, put},
    Router,
};
use overwatch_rs::services::state::NoState;
use parking_lot::Mutex;
use std::future::Future;

// internal
use super::HttpBackend;
use crate::HttpMethod;

/// Configuration for the Http Server
#[derive(Debug, Clone, clap::Args, serde::Deserialize, serde::Serialize)]
pub struct ServerSettings {
    /// Socket where the server will be listening on for incoming requests.
    #[arg(short, long = "addr", default_value_t = std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)), 8080), env = "HTTP_ADDRESS")]
    pub address: std::net::SocketAddr,
    /// Allowed origins for this server deployment requests.
    #[arg(long = "cors-origin")]
    pub cors_origins: Vec<String>,
}

pub trait HandlerOutput<T>: Future<Output = T> + Sized + Send {}

#[derive(Clone)]
pub struct AxumBackend {
    get_handles: Arc<Mutex<HashMap<String, <Self as HttpBackend>::Handler>>>,
    post_handlers: Arc<Mutex<HashMap<String, <Self as HttpBackend>::Handler>>>,
    patch_handlers: Arc<Mutex<HashMap<String, <Self as HttpBackend>::Handler>>>,
    put_handlers: Arc<Mutex<HashMap<String, <Self as HttpBackend>::Handler>>>,
    delete_handlers: Arc<Mutex<HashMap<String, <Self as HttpBackend>::Handler>>>,
    config: ServerSettings,
}

async fn handle_get_plugins(
    Extension(backend): Extension<AxumBackend>,
    Path(path): Path<String>,
) -> impl IntoResponse + Send + Sync + 'static {
    match backend.get_handles.lock().get(&path) {
        Some(f) => Ok(f(()).await),
        None => Err(()),
    }
}

async fn handle_post_plugins(
    Extension(backend): Extension<AxumBackend>,
    Path(path): Path<String>,
) -> impl IntoResponse + Send + Sync + 'static {
    todo!()
}

async fn handle_patch_plugins(
    Extension(backend): Extension<AxumBackend>,
    Path(path): Path<String>,
) -> impl IntoResponse + Send + Sync + 'static {
    todo!()
}

async fn handle_put_plugins(
    Extension(backend): Extension<AxumBackend>,
    Path(path): Path<String>,
) -> impl IntoResponse + Send + Sync + 'static {
    todo!()
}

async fn handle_delete_plugins(
    Extension(backend): Extension<AxumBackend>,
    Path(path): Path<String>,
) -> impl IntoResponse + Send + Sync + 'static {
    todo!()
}

#[async_trait::async_trait]
impl HttpBackend for AxumBackend {
    type Config = ServerSettings;

    type State = NoState<ServerSettings>;

    type Request = ();

    type Response = String;

    type Handler =
        Box<dyn Send + Sync + Fn(Self::Request) -> Pin<Box<dyn Future<Output = Self::Response>>>>;

    fn new(config: Self::Config) -> Result<Self, overwatch_rs::DynError>
    where
        Self: Sized,
    {
        Ok(Self {
            config,
            get_handles: Arc::new(Mutex::new(HashMap::new())),
            post_handlers: Arc::new(Mutex::new(HashMap::new())),
            patch_handlers: Arc::new(Mutex::new(HashMap::new())),
            put_handlers: Arc::new(Mutex::new(HashMap::new())),
            delete_handlers: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn add_route(&self, service_id: overwatch_rs::services::ServiceId, route: crate::Route<Self>) {
        match route.method {
            HttpMethod::GET => {
                self.get_handles.lock().insert(route.path, route.handler);
            }
            HttpMethod::POST => {
                self.post_handlers.lock().insert(route.path, route.handler);
            }
            HttpMethod::PATCH => {
                self.patch_handlers.lock().insert(route.path, route.handler);
            }
            HttpMethod::PUT => {
                self.put_handlers.lock().insert(route.path, route.handler);
            }
            HttpMethod::DELETE => {
                self.delete_handlers
                    .lock()
                    .insert(route.path, route.handler);
            }
        }
    }

    async fn run(&self) -> Result<(), overwatch_rs::DynError> {
        let router = Router::new()
            .route("/plugins/get/:path", get(handle_get_plugins))
            .route("/plugins/post/:path", post(handle_post_plugins))
            .route("/plugins/patch/:path", patch(handle_patch_plugins))
            .route("/plugins/put/:path", put(handle_put_plugins))
            .route("/plugins/delete/:path", delete(handle_delete_plugins));

        axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
            .serve(router.into_make_service())
            .await?;
        Ok(())
    }
}
