//! oxbow

#![feature(generic_associated_types, adt_const_params)]
#![allow(incomplete_features)]

#[macro_use]
extern crate lalrpop_util;

pub mod bot;
pub mod msg;
pub mod parse;
pub mod store;
pub mod wordsearch;
