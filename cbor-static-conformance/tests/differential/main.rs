//! The CBOR data model against three independent readers: ciborium's `Value`, minicbor's
//! Tokenizer and Python's cbor2 each read generated well-formed CBOR as cbor-static's `Tokens`
//! does, after the normalization of that reader's leg.

#![cfg(not(miri))]

#[path = "../common/mod.rs"]
mod common;

#[path = "../../../cbor-static/src/well_formed/tree.rs"]
mod tree;

mod data_model;
mod normalization;
