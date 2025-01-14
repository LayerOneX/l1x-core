use crate::execute_common::ContractExecutor;
use async_trait::async_trait;

pub struct ExecuteContract;

#[async_trait]
impl<'a, 'g> ContractExecutor<'a, 'g> for ExecuteContract {}
