//! Optional database integrations.
//!
//! Both adapters carry [`Money`](crate::Money) and [`Rate`](crate::Rate) as
//! canonical text. They share one private encode/decode edge, so driver choice
//! cannot change the monetary representation.

#[cfg(any(feature = "postgres", feature = "sqlx"))]
mod codec;

/// `postgres-types` integration.
#[cfg(feature = "postgres")]
#[cfg_attr(docsrs, doc(cfg(feature = "postgres")))]
pub mod postgres;

/// SQLx PostgreSQL integration.
#[cfg(feature = "sqlx")]
#[cfg_attr(docsrs, doc(cfg(feature = "sqlx")))]
pub mod sqlx;
