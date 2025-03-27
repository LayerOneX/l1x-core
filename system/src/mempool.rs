use std::error::Error;
use tokio::sync::oneshot;
use crate::transaction::Transaction;

#[derive(Debug)]
pub enum ProcessMempool {
	AddTransaction(Transaction, oneshot::Sender<Result<ResponseMempool, Box<dyn Error + Send>>>),
	RemoveTrasaction(Transaction),
	ProposeBlockOnMempoolFull,
	ProposeBlockOnBlockTime,
	GetSize(oneshot::Sender<usize>),
}

#[derive(Debug, Clone)]
pub enum ResponseMempool {
	Success,
	FailedToAddTransaction,
}
