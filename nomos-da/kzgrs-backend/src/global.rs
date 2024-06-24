use kzgrs::{global_parameters_from_randomness, GlobalParameters};
use once_cell::sync::Lazy;

pub static GLOBAL_PARAMETERS: Lazy<GlobalParameters> = Lazy::new(|| {
    let mut rng = rand::thread_rng();
    global_parameters_from_randomness(&mut rng)
});
