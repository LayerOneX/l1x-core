use crate::validate_common::ContractValidator;
use async_trait::async_trait;

pub struct ValidateContract;

#[async_trait]
impl<'a> ContractValidator<'a> for ValidateContract {}
