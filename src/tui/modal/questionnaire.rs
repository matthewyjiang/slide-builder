#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Question {
    pub id: String,
    pub prompt: String,
    pub choices: Vec<String>,
    pub allow_free_text: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestionnaireState {
    pub title: String,
    pub questions: Vec<Question>,
    pub active: usize,
    pub answer: String,
    pub selected_choice: usize,
}
