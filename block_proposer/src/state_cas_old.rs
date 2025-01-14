use anyhow::{anyhow, Error};
use db::{
    cassandra::DatabaseManager,
    utils::{FromByteArray, ToByteArray},
};
use log::info;
use primitives::{arithmetic::ScalarBig, *};
use scylla::{Session, _macro_internal::CqlValue};
use std::{collections::HashMap, sync::Arc};
use system::block_proposer::BlockProposer;

pub struct BlockProposerState {
    pub session: Arc<Session>,
}

impl BlockProposerState {
    pub async fn new() -> Result<Self, Error> {
        let db_session = DatabaseManager::get_session().await?;
        let block_proposer_state = BlockProposerState { session: db_session.clone() };
        block_proposer_state.create_table().await?;
        Ok(block_proposer_state)
    }

    pub async fn get_state(session: Session) -> Result<Self, Error> {
        let db_session = DatabaseManager::get_session().await?;
        let state = BlockProposerState { session: db_session.clone() };
        state.create_table().await?;
        Ok(state)
    }

    pub async fn create_table(&self) -> Result<(), Error> {
        match self
            .session
            .query(
                "CREATE TABLE IF NOT EXISTS block_proposer (
                    cluster_address blob,
                    block_number Bigint,
                    address blob,
					selected_next boolean,
                    PRIMARY KEY (address, cluster_address, block_number)
                );",
                &[],
            )
            .await
        {
            Ok(_) => {},
            Err(e) => return Err(anyhow!("Create table failed: {}", e)),
        };
        Ok(())
    }

    pub async fn store_block_proposer(
        &self,
        cluster_address: Address,
        block_number: BlockNumber,
        address: Address,
    ) -> Result<(), Error> {
        let block_number: i64 = i64::try_from(block_number).unwrap_or(i64::MAX);
        match self
            .session
            .query(
                "INSERT INTO block_proposer (cluster_address, block_number, address, selected_next) VALUES (?, ?, ?, ?);",
                (&cluster_address, &block_number, &address, &false),
            )
            .await
        {
            Ok(_) => {},
            Err(e) => return Err(anyhow!("Failed to store block proposer - {}", e)),
        };
        Ok(())
    }

    pub async fn load_selectors_block_numbers(
        &self,
        address: &Address,
    ) -> Result<Option<Vec<BlockNumber>>, Error> {
        let rows = self
            .session
            .query(
                "SELECT address, block_number FROM block_proposer WHERE address = ? allow filtering;",
                (address,),
            )
            .await?
            .rows()?;

        let mut block_numbers = Vec::new();
        for r in rows {
            let (_address, block_number) = r.into_typed::<(Address, i64)>()?;
            block_numbers.push(block_number.try_into().unwrap_or(BlockNumber::MIN));
        }
        if block_numbers.is_empty() {
            Ok(None)
        } else {
            Ok(Some(block_numbers))
        }
    }

    pub async fn update_selected_next(
        &self,
        selector_address: &Address,
        cluster_address: &Address,
    ) -> Result<(), Error> {
        if let Some(block_numbers) = self.load_selectors_block_numbers(selector_address).await? {
            for block_number in block_numbers {
                let block_number: i64 = i64::try_from(block_number).unwrap_or(i64::MAX);
                match self
                    .session
                    .query(
                        "UPDATE block_proposer SET selected_next = ? WHERE address = ? and cluster_address = ? and block_number = ?;",
                        (&true, selector_address, cluster_address, &block_number),
                    )
                    .await
                {
                    Ok(_) => {},
                    Err(e) => return Err(anyhow!("Failed to update block proposer - {}", e)),
                }
            }
        }
        Ok(())
    }
    pub async fn store_block_proposers(
        &self,
        block_proposers: &HashMap<Address, HashMap<BlockNumber, Address>>,
        selector_address: Option<Address>,
        cluster_address: Option<Address>,
    ) -> Result<(), Error> {
        for (cluster_address, block_numbers) in block_proposers {
            for (block_number, address) in block_numbers {
                self.store_block_proposer(cluster_address.clone(), *block_number, address.clone())
                    .await?;
            }
        }
        if let Some(selector_address) = selector_address {
            if let Some(cluster_address) = cluster_address {
                self.update_selected_next(&selector_address, &cluster_address).await?;
            }
        }
        Ok(())
    }

    pub async fn load_last_block_proposer_block_number(
        &self,
        cluster_address: Address,
    ) -> Result<BlockNumber, Error> {
        let (block_number,)= self
            .session
            .query(
                "SELECT MAX(block_number) FROM block_proposer WHERE cluster_address = ? allow filtering;",
                (&cluster_address,),
            )
            .await?
            .single_row()?
            .into_typed::<(i64,)>()?;

        let block_number = block_number.try_into().unwrap_or(BlockNumber::MIN);

        Ok(block_number)
    }

    pub async fn load_block_proposer(
        &self,
        cluster_address: Address,
        block_number: BlockNumber,
    ) -> Result<Option<BlockProposer>, Error> {
        let block_number1: i64 = i64::try_from(block_number).unwrap_or(i64::MAX);
        let query_result = match self
            .session
            .query(
                "SELECT address FROM block_proposer WHERE cluster_address = ? AND block_number = ? allow filtering;",
                (&cluster_address, &block_number1),
            )
            .await
        {
            Ok(q) => q,
            Err(e) => return Err(anyhow!("Failed to load block proposer - {}", e)),
        };

        let (address_idx, _) = query_result
            .get_column_spec("address")
            .ok_or_else(|| anyhow!("No address column found"))?;

        let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

        let address: Address = if let Some(row) = rows.get(0) {
            if let Some(address_value) = &row.columns[address_idx] {
                if let CqlValue::Blob(address) = address_value {
                    address.as_slice().try_into().map_err(|_| anyhow!("Invalid length"))?
                } else {
                    return Err(anyhow!("Unable to convert to Address type"))
                }
            } else {
                return Err(anyhow!("Unable to read block_number column"))
            }
        } else {
            return Err(anyhow!("block_proposer_state : address 167: Unable to read row"))
        };

        Ok(Some(BlockProposer { cluster_address, block_number, address }))
    }

    pub async fn load_block_proposers(
        &self,
        cluster_address: &Address,
    ) -> Result<HashMap<BlockNumber, BlockProposer>, Error> {
        let rows = self
            .session
            .query("SELECT cluster_address, block_number, address FROM block_proposer WHERE cluster_address = ? allow filtering;", (cluster_address,))
            .await?
            .rows()?;

        let mut block_proposers = HashMap::new();
        for row in rows {
            let (cluster_address, block_number, address) =
                row.into_typed::<(Address, i64, Address)>()?;
            let block_number = block_number.try_into().unwrap_or(BlockNumber::MIN);
            block_proposers
                .insert(block_number, BlockProposer { cluster_address, block_number, address });
        }
        Ok(block_proposers)
    }

    pub async fn find_cluster_address(&self, address: Address) -> Result<Address, Error> {
        let query_result = match self
            .session
            .query(
                "SELECT cluster_address FROM block_proposer WHERE address = ? ALLOW FILTERING;",
                (&address,),
            )
            .await
        {
            Ok(q) => q,
            Err(e) => return Err(anyhow!("Failed to find full node info - {}", e)),
        };

        let (cluster_address_idx, _) = query_result
            .get_column_spec("cluster_address")
            .ok_or_else(|| anyhow!("No cluster_address column found"))?;

        let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

        if let Some(row) = rows.get(0) {
            let cluster_address = if let Some(cluster_address_value) =
                &row.columns[cluster_address_idx]
            {
                if let CqlValue::Blob(cluster_address) = cluster_address_value {
                    cluster_address.as_slice().try_into().map_err(|_| anyhow!("Invalid length"))?
                } else {
                    return Err(anyhow!("Unable to convert to Address type"))
                }
            } else {
                return Err(anyhow!("Unable to read cluster_address column"))
            };
            Ok(cluster_address)
        } else {
            return Err(anyhow!("block_proposer_state:  cluster_address 276 : Unable to read row"))
        }
    }

    pub async fn is_selected_next(&self, address: &Address) -> Result<bool, Error> {
        let (count,) = self
            .session
            .query("SELECT COUNT(*) AS count FROM block_proposer WHERE address = ? AND selected_next = false allow filtering;", (address,))
            .await?
            .single_row()?
            .into_typed::<(i64,)>()?;

        Ok(!(count > 0))
    }
}