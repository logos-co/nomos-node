pub trait Metadata {
    type AppId;
    type Index;

    fn metadata(&self) -> Option<(Self::AppId, Self::Index)>;
}
