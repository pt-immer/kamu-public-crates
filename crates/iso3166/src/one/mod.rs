//! ISO 3166-1 types: [`Alpha2`], [`Alpha3`], [`Numeric`].

mod generated {
    #![allow(clippy::all, clippy::pedantic, clippy::nursery)]
    include!(concat!(env!("OUT_DIR"), "/one_generated.rs"));
}

pub use generated::{Alpha2, Alpha3};

mod alpha2;
mod alpha3;
mod numeric;

pub use numeric::Numeric;

pub(crate) use generated::{
    ALPHA2_BY_STR, ALPHA3_BY_STR, numeric_to_alpha2_generated, numeric_to_alpha3_generated,
};
