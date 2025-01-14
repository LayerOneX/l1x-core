use anyhow::{Context, Error};
use db::cassandra::DatabaseManager;
use primitives::*;
use scylla::Session;
use std::sync::Arc;
use system::contract::Contract;

pub struct ContractState {
    pub session: Arc<Session>,
}

impl ContractState {
    pub async fn new() -> Result<Self, Error> {
        let db_session = DatabaseManager::get_session().await?;
        let state = ContractState { session: db_session.clone() };
        state
            .create_table()
            .await
            .with_context(|| "Failed to create new ContractState")?;
        Ok(state)
    }

    pub async fn create_table(&self) -> Result<(), Error> {
        self.session
            .query(
                "CREATE TABLE IF NOT EXISTS contract (
                    address blob,
                    access tinyint,
                    type tinyint,
                    code blob,
                    owner_address blob,
                    PRIMARY KEY (address)
                );",
                &[],
            )
            .await
            .with_context(|| "Failed to create contract table")?;

        Ok(())
    }

    pub async fn store_contract(&self, contract: &Contract) -> Result<(), Error> {
        self.session
            .query(
                "INSERT INTO contract (address, access, type, code, owner_address) VALUES (?, ?, ?, ?, ?)",
                (
                    &contract.address,
                    &contract.access,
                    &contract.r#type,
                    &contract.code,
                    &contract.owner_address,
                ),
            )
            .await
            .with_context(|| "Failed to store contract data")?;
        Ok(())
    }

    pub async fn update_contract_code(&self, contract: &Contract) -> Result<(), Error> {
        self.session
            .query(
                "UPDATE contract SET code = ? WHERE address = ?;",
                (&contract.code, &contract.address),
            )
            .await
            .with_context(|| {
                format!("Failed to update for provided address {:?}", contract.address)
            })?;
        Ok(())
    }

    pub async fn get_all_contract(&self) -> Result<Vec<Contract>, Error> {
        let rows = self
            .session
            .query("SELECT address, access, type, code, owner_address FROM contract;", &[])
            .await?
            .rows()?;

        let mut contracts = vec![];
        for r in rows {
            let (address, access, r#type, code, owner_address) =
                r.into_typed::<(Address, AccessType, ContractType, Vec<u8>, Address)>()?;
            contracts.push(Contract { address, access, r#type, code, owner_address });
        }

        Ok(contracts)
    }

    pub async fn get_contract(&self, address: &Address) -> Result<Contract, Error> {
        let (access, r#type, code, owner_address) = self
            .session
            .query(
                "SELECT access, type, code, owner_address FROM contract WHERE address = ?;",
                (&address,),
            )
            .await?
            .single_row()?
            .into_typed::<(AccessType, ContractType, Vec<u8>, Address)>()?;

        Ok(Contract { address: *address, access, r#type, code, owner_address })
    }

    pub async fn get_contract_owner(
        &self,
        address: &Address,
    ) -> Result<(AccessType, Address), Error> {
        let (access, owner_address) = self
            .session
            .query("SELECT access, owner_address FROM contract WHERE address = ?;", (&address,))
            .await?
            .single_row()
            .with_context(|| format!("Failed to find unique contract for address {:?}", address))?
            .into_typed::<(AccessType, Address)>()?;

        Ok((access, owner_address))
    }

    pub async fn is_valid_contract(&self, address: &Address) -> Result<bool, Error> {
        let (count,) = self
            .session
            .query("SELECT COUNT(*) AS count FROM contract WHERE address = ?;", (&address,))
            .await?
            .single_row()?
            .into_typed::<(i64,)>()?;

        Ok(count > 0)
    }
}