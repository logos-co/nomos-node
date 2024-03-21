#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    // The k parameter in the Common Prefix property.
    // Blocks deeper than k are generally considered stable and forks deeper than that
    // trigger the additional fork selection rule, which is however only expected to be used
    // during bootstrapping.
    pub security_param: u32,
    // f, the rate of occupied slots
    pub active_slot_coeff: f64,
}

impl Config {
    pub fn base_period_length(&self) -> u64 {
        (f64::from(self.security_param) / self.active_slot_coeff).floor() as u64
    }

    // return the number of slots required to have great confidence at least k blocks have been produced
    pub fn s(&self) -> u64 {
        self.base_period_length() * 3
    }
}
