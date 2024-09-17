use rand::{rngs::mock::StepRng, RngCore, SeedableRng};

pub struct TestRng(StepRng);

/// Implement RngCore for TestRng
impl RngCore for TestRng {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.0.try_fill_bytes(dest)
    }
}

impl SeedableRng for TestRng {
    type Seed = [u8; 8];

    fn from_seed(seed: Self::Seed) -> Self {
        let seed_as_u64 = u64::from_le_bytes(seed);
        TestRng(StepRng::new(seed_as_u64, 1))
    }

    fn seed_from_u64(seed: u64) -> Self {
        TestRng(StepRng::new(seed, 1))
    }
}
