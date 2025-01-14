#[derive(Clone, Copy)]
pub struct MemSlice {
	pub addr: u64,
	pub len: u64,
}

impl MemSlice {
	#[inline]
	pub fn len<T: TryFrom<u64>>(&self) -> Result<T, ()> {
		T::try_from(self.len).map_err(|_| ())
	}

	#[inline]
	#[allow(dead_code)]
	pub fn end<T: TryFrom<u64>>(&self) -> Result<T, ()> {
		T::try_from(self.addr.checked_add(self.len).ok_or(())?).map_err(|_| ())
	}

	#[inline]
	#[allow(dead_code)]
	pub fn range<T: TryFrom<u64>>(&self) -> Result<std::ops::Range<T>, ()> {
		let end = self.end()?;
		let start = T::try_from(self.addr).map_err(|_| ())?;
		Ok(start..end)
	}
}
