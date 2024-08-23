pub mod dispersal;
pub mod replication;
pub mod sampling;

use bincode::ErrorKind;

fn clone_deserialize_error(error: &bincode::Error) -> bincode::Error {
    Box::new(match error.as_ref() {
        ErrorKind::Io(error) => ErrorKind::Io(std::io::Error::new(error.kind(), error.to_string())),
        ErrorKind::InvalidUtf8Encoding(error) => ErrorKind::InvalidUtf8Encoding(*error),
        ErrorKind::InvalidBoolEncoding(bool) => ErrorKind::InvalidBoolEncoding(*bool),
        ErrorKind::InvalidCharEncoding => ErrorKind::InvalidCharEncoding,
        ErrorKind::InvalidTagEncoding(tag) => ErrorKind::InvalidTagEncoding(*tag),
        ErrorKind::DeserializeAnyNotSupported => ErrorKind::DeserializeAnyNotSupported,
        ErrorKind::SizeLimit => ErrorKind::SizeLimit,
        ErrorKind::SequenceMustHaveLength => ErrorKind::SequenceMustHaveLength,
        ErrorKind::Custom(custom) => ErrorKind::Custom(custom.clone()),
    })
}
