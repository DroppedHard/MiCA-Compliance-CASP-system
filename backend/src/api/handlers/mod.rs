//! Handlery HTTP pogrupowane według odbiorcy i przypadku użycia.

pub(super) mod administration;
pub(super) mod customer;
pub(super) mod public;

pub(super) use administration::*;
pub(super) use customer::*;
pub(super) use public::*;
