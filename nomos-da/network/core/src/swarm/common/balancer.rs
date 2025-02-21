pub trait SubnetworkConnectionPolicy {
    type Stats;
    type Output;

    fn connection_number_deviation(&self, stats: &Self::Stats) -> Self::Output;
}
