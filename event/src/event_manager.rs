use crate::event_state::EventState;
use anyhow::Error;
use system::event::Event;

pub struct EventManager {
	pub event: Event,
}

impl<'a> EventManager {
	pub async fn new(event: &Event, event_state: &EventState<'a>) -> Result<EventManager, Error> {
		event_state.create_event(event).await?;
		let event_manager = EventManager { event: event.clone() };
		Ok(event_manager)
	}
}
