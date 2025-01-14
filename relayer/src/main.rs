use ethers::{
	core::types::{Address, Filter, H160, H256},
	prelude::*,
	providers::{Middleware, Provider},
};
use log::info;

// const HTTP_URL: &str = "https://rpc.flashbots.net";
const WSS_URL: &str = "wss://mainnet.infura.io/ws/v3/bce5dbb7e35c4d039a87c8e43a5cef73";
const L1X_URL: &str = "ws://127.0.0.1:50051";
const ETH_USDC_V3_CONTRACT: &str = "0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640";

// const V3FACTORY_ADDRESS: &str = "0x1F98431c8aD98523631AE4a59f267346ea31F984";
// const DAI_ADDRESS: &str = "0x6B175474E89094C44Da98b954EedeAC495271d0F";
// const USDC_ADDRESS: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
// const USDT_ADDRESS: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	pretty_env_logger::init();
	// let ethereum = Provider::<Ws>::connect(WSS_URL).await?;
	let l1x = Provider::<Ws>::connect(L1X_URL).await?;

	// let block_num = ethereum.get_block_number().await?;

	// let client = Arc::new(provider);
	// let token_topics = vec![
	//     H256::from(USDC_ADDRESS.parse::<H160>()?),
	//     H256::from(USDT_ADDRESS.parse::<H160>()?),
	//     H256::from(DAI_ADDRESS.parse::<H160>()?),
	// ];

	// let filter = Filter::new()
	// .address(ETH_USDC_V3_CONTRACT.parse::<Address>()?)
	// .event("PoolCreated(address,address,uint24,int24,address)")
	// .event("TokensLocked(address,address,uint24,int24,address)")
	// .topic1(token_topics.to_vec())
	// .topic2(token_topics.to_vec())
	// .from_block(block_num);

	let filter = Filter::new()
		.address("028c114ed637ef1f624ffeeaa2cc00fc4daa1549".parse::<Address>()?)
		.event("Event1(address,string,address,uint256)")
		.topic1(H256::from("0x75104938baa47c54a86004ef998cc76c2e616289".parse::<H160>()?)) // msg.sender
		.topic2(
			"0x000000000000000000000000000000000000000000000000000000000000000a".parse::<H256>()?,
		)
		.from_block(55)
		.to_block(100);

	let filter = Filter::new();

	let result_logs = l1x.get_logs(&filter).await.unwrap();

	if result_logs.is_empty() {
		println!("No logs found...");
		return Ok(())
	}

	for log in result_logs.iter() {
		println!("{:?}", log);
	}

	println!("\n-------------------\n");

	let mut stream = l1x.subscribe_logs(&filter).await?;
	// let mut stream = ethereum.subscribe_logs(&filter).await?;

	let mut log_count = 0;
	while let Some(log) = stream.next().await {
		info!("LOG COUNT: {log_count}");
		log_count += 1;
		println!("{:?}", log);
	}

	// let logs = client.get_logs(&filter).await?;
	// println!("{} pools found!", logs.iter().len());
	// for log in logs.iter() {
	//     let token0 = Address::from(log.topics[1]);
	//     let token1 = Address::from(log.topics[2]);
	//     let fee_tier = U256::from_big_endian(&log.topics[3].as_bytes()[29..32]);
	//     let tick_spacing = U256::from_big_endian(&log.data[29..32]);
	//     let pool = Address::from(&log.data[44..64].try_into()?);
	//     println!(
	//         "pool = {pool}, token0 = {token0}, token1 = {token1}, fee = {fee_tier}, spacing =
	// {tick_spacing}"     );
	// }

	Ok(())
}
