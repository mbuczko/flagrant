// use axum::{
//     extract::{FromRef, FromRequestParts},
//     http::request::Parts,
//     RequestPartsExt,
// };
// use axum_extra::{headers, TypedHeader};
// use sqlx::{Pool, Sqlite};

// use crate::{cookie::Cookie, errors};

// impl<S> FromRequestParts<S> for Cookie
// where
//     Pool<Sqlite>: FromRef<S>,
//     S: Send + Sync,
// {
//     type Rejection = errors::ServiceError;

//     async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
//         let cookies = parts
//             .extract::<TypedHeader<headers::Cookie>>()
//             .await
//             .map_err(|err| err.into());
//     }

//     // https://github.com/tokio-rs/axum/blob/b6b203b3065e4005bda01efac8429176da055ae2/examples/oauth/src/main.rs#L127
//     //
// }
