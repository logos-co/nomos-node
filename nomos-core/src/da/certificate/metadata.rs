pub trait Metadata {
    type AppId;
    type Index;

    fn metadata(&self) -> (Self::AppId, Self::Index);
}
