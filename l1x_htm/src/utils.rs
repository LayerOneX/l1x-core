#[derive(Debug, Clone)]
pub struct Column {
    pub cells: Vec<Cell>,
}

impl Column {
    pub fn new(cells_per_column: usize) -> Self {
        Column {
            cells: (0..cells_per_column).map(|_| Cell::new()).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub segments: Vec<Segment>,
    pub predictive: bool,
}

impl Cell {
    pub fn new() -> Self {
        Cell {
            segments: Vec::new(),
            predictive: false,
        }
    }

    pub fn is_predictive(&self, active_cells: &[usize]) -> bool {
        self.segments.iter().any(|segment| segment.is_active(active_cells))
    }

    pub fn reinforce_active_synapses(&mut self) {
        for segment in &mut self.segments {
            segment.reinforce_active_synapses();
        }
    }

    pub fn reinforce_predictive_synapses(&mut self, active_cells: &[usize]) {
        for segment in &mut self.segments {
            if segment.is_active(active_cells) {
                segment.reinforce_active_synapses();
            }
        }
    }

    pub fn grow_new_synapses(&mut self, active_cells: &[usize]) {
        if self.segments.is_empty() {
            self.segments.push(Segment::new());
        }
        self.segments[0].grow_new_synapses(active_cells);
    }
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub synapses: Vec<Synapse>,
}

impl Segment {
    pub fn new() -> Self {
        Segment {
            synapses: Vec::new(),
        }
    }

    pub fn is_active(&self, active_cells: &[usize]) -> bool {
        self.synapses.iter().filter(|s| s.is_connected() && active_cells.contains(&s.target_cell)).count() >= 3
    }

    pub fn reinforce_active_synapses(&mut self) {
        for synapse in &mut self.synapses {
            synapse.reinforce();
        }
    }

    pub fn grow_new_synapses(&mut self, active_cells: &[usize]) {
        for &cell in active_cells.iter().take(5) {
            if !self.synapses.iter().any(|s| s.target_cell == cell) {
                self.synapses.push(Synapse::new(cell));
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Synapse {
    pub target_cell: usize,
    pub permanence: f64,
}

impl Synapse {
    pub fn new(target_cell: usize) -> Self {
        Synapse {
            target_cell,
            permanence: 0.21,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.permanence > 0.2
    }

    pub fn reinforce(&mut self) {
        self.permanence = (self.permanence + 0.1).min(1.0);
    }
}

pub fn compute_anomaly(active_columns: &[bool], predicted_columns: &[bool]) -> f64 {
    let active_count = active_columns.iter().filter(|&&x| x).count();
    let predicted_count = predicted_columns.iter().filter(|&&x| x).count();
    let overlap_count = active_columns.iter().zip(predicted_columns.iter())
        .filter(|&(&a, &p)| a && p).count();

    if active_count == 0 {
        return 0.0;
    }

    // Use both active_count and predicted_count in the calculation
    let false_positives = predicted_count.saturating_sub(overlap_count);
    let false_negatives = active_count.saturating_sub(overlap_count);
    
    (false_positives + false_negatives) as f64 / (active_count + predicted_count) as f64
}