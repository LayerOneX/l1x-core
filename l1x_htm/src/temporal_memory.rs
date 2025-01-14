use super::utils::Column;

#[derive(Debug, Clone)]
pub struct TemporalMemory {
	columns: Vec<Column>,
	active_cells: Vec<usize>,
	predictive_cells: Vec<usize>,
}

impl TemporalMemory {
	pub fn new(num_columns: usize, cells_per_column: usize) -> Self {
		TemporalMemory {
			columns: (0..num_columns).map(|_| Column::new(cells_per_column)).collect(),
			active_cells: Vec::new(),
			predictive_cells: Vec::new(),
		}
	}

	pub fn compute(&mut self, active_columns: &[bool], learn: bool) -> (Vec<usize>, Vec<usize>) {
		self.activate_cells(active_columns, learn);
		self.predict_cells(learn);
		(self.active_cells.clone(), self.predictive_cells.clone())
	}

	fn activate_cells(&mut self, active_columns: &[bool], learn: bool) {
		self.active_cells.clear();
		for (i, &active) in active_columns.iter().enumerate() {
			if active {
				let mut burst = true;
				let mut column_active_cells = Vec::new();
				let cells_per_column = self.columns[i].cells.len();

				for j in 0..cells_per_column {
					if self.columns[i].cells[j].predictive {
						column_active_cells.push(i * cells_per_column + j);
						burst = false;
					}
				}

				if burst {
					for j in 0..cells_per_column {
						column_active_cells.push(i * cells_per_column + j);
					}
				}

				self.active_cells.extend(column_active_cells.iter());

				if learn {
					for &cell_index in &column_active_cells {
						let column_index = cell_index / cells_per_column;
						let cell_in_column_index = cell_index % cells_per_column;
						if burst {
							self.columns[column_index].cells[cell_in_column_index].grow_new_synapses(&self.active_cells);
						} else {
							self.columns[column_index].cells[cell_in_column_index].reinforce_active_synapses();
						}
					}
				}
			}
		}
	}

	fn predict_cells(&mut self, learn: bool) {
		self.predictive_cells.clear();
		for (i, column) in self.columns.iter_mut().enumerate() {
			let cells_per_column = column.cells.len();
			for j in 0..cells_per_column {
				let cell_index = i * cells_per_column + j;
				let is_predictive = column.cells[j].is_predictive(&self.active_cells);
				
				if is_predictive {
					self.predictive_cells.push(cell_index);
					column.cells[j].predictive = true;
					if learn {
						column.cells[j].reinforce_predictive_synapses(&self.active_cells);
					}
				} else {
					column.cells[j].predictive = false;
				}
			}
		}
	}
}