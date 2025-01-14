mod config_cache;
mod config_v1;
mod deny_config_cache;
mod staking_contract_cache;
pub use config_cache::{KinScoreConfig, RuntimeConfigCache, StakeScoreConfig, ValidatorRewards, XscoreConfig};
pub use deny_config_cache::RuntimeDenyConfigCache;
pub use deny_config_cache::WBAddresses;
pub use staking_contract_cache::NodePoolInfo;
pub use staking_contract_cache::RuntimeStakingInfoCache;

use anyhow::{anyhow, Error};
use base64::Engine;
use primitives::Address;
use serde::{Deserialize, Deserializer};
use std::fmt;

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct AddressString(pub Address);

impl fmt::Display for AddressString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&hex::encode(self.0), f)
    }
}

impl fmt::Debug for AddressString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&hex::encode(self.0), f)
    }
}

impl TryFrom<String> for AddressString {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl TryFrom<&String> for AddressString {
    type Error = String;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl TryFrom<&str> for AddressString {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let value = value.strip_prefix("0x").unwrap_or(value);
        match hex::decode(value) {
            Ok(val) => match <Address>::try_from(val) {
                Ok(address) => Ok(Self(address)),
                Err(_) => Err(format!("Can't create address from vector")),
            },
            Err(_) => Err(format!("Can't create address from string {}", value)),
        }
    }
}

impl<'de> Deserialize<'de> for AddressString {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        Ok(AddressString::try_from(s).map_err(|err| serde::de::Error::custom(err))?)
    }
}

fn try_unquote_string_fast<'a>(data: &'a Vec<u8>) -> Result<String, Error> {
    if data.len() < 2 {
        return Err(anyhow!("Data is too short"));
    }
    let first_char = data.first().ok_or(anyhow!("Can't get first char"))?;
    let last_char = data.last().ok_or(anyhow!("Can't get last char"))?;

    if *first_char == '"' as u8 {
        if *last_char == '"' as u8 {
            Ok(String::from_utf8_lossy(&data[1..data.len() - 1]).to_string())
        } else {
            Err(anyhow!("Last char is not \"'\""))
        }
    } else {
        Err(anyhow!("First char is not \"'\""))
    }
}

pub(crate) fn decode_base64(data: &Vec<u8>) -> Result<Vec<u8>, Error> {
    let data = if let Ok(data) = try_unquote_string_fast(data) {
        data
    } else {
        serde_json::from_slice(&data).map_err(|e| anyhow!("Can't decode Runtime Config from json: {e}"))?
    };

    let mut buffer = Vec::new();
    base64::engine::general_purpose::STANDARD
        .decode_vec(data, &mut buffer)
        .map_err(|e| anyhow!("Can't decode Runtime Config from base64: {e}"))?;

    Ok(buffer)
}
