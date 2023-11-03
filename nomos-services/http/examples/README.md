# Http service examples

## Axum.rs
A simple service to demonstrate how to register http handler for overwatch service.

To run this example use:
```bash
cargo run --example axum --features http
```

A GET endpoint will be registered at `http://localhost:8080/dummy/`. An endpoint corresponds with the Service name.

## Graphql.rs
A demonstration of usage from within an overwatch service over the http.

To run this example use:
```bash
cargo run --example graphql --features http,gql
```

An endpoint will be registered at `http://localhost:8080/dummygraphqlservice/`. An endpoint corresponds with the Service name.

To query this endpoint use:
```bash
curl --location --request POST 'localhost:8080/dummygraphqlservice/' \
--data-raw '{"query":"query {val}","variables":{}}'

```

Every response should increment the `val` variable.
```json
{"data":{"val":1}}
```
