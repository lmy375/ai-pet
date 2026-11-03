//! What the run loop must do after applying an event.

pub enum Action {
    /// Dispatch this line to the command/chat handler (sets busy).
    Submit(String),
    /// The viewed session changed: attach to its running turn, if any.
    SyncTurn,
    /// Build and open the tasks picker (the run loop owns the shell store).
    OpenTasks,
    /// Fetch the model list from the API and open the model picker.
    OpenModels,
    /// Build and open the group-members picker (needs the async group state).
    OpenMembers,
    /// Apply the member set picked in the overlay (sets busy).
    SetMembers(Vec<String>),
    Quit,
}