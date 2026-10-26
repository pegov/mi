use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct CreditsData {
    total_credits: f64,
    total_usage: f64,
}

#[derive(Deserialize, Debug)]
struct CreditsResponse {
    data: CreditsData,
}

#[derive(Default)]
pub struct Session {
    pub start_balance: f64,
    pub curr_balance: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
    pub active_context_tokens: u64,
    pub context_ratio: f64,
    pub pending_summaries: Vec<std::sync::mpsc::Receiver<crate::compaction::SummaryOutcome>>,
}

impl Session {
    fn new(start_balance: f64) -> Self {
        Self {
            start_balance,
            curr_balance: start_balance,
            input_tokens: 0,
            output_tokens: 0,
            cache_tokens: 0,
            active_context_tokens: 0,
            context_ratio: 1.0,
            pending_summaries: vec![],
        }
    }

    pub fn collect_summary_usage(&mut self) {
        for index in (0..self.pending_summaries.len()).rev() {
            match self.pending_summaries[index].try_recv() {
                Ok(outcome) => {
                    if let Some(usage) = outcome.usage {
                        self.save_usage(&usage);
                    }
                    self.pending_summaries.swap_remove(index);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending_summaries.swap_remove(index);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    pub fn estimate_context(&self, tokens: u64) -> u64 {
        (tokens as f64 * self.context_ratio.max(1.0)).ceil() as u64
    }

    pub fn save_usage(&mut self, usage: &crate::completions::Usage) {
        self.save_tokens(
            usage.prompt_tokens,
            usage.completion_tokens,
            usage.prompt_tokens_details.cached_tokens,
        );
        if let Some(cost) = usage.cost {
            self.save_cost(cost);
        }
    }

    pub fn save_tokens(&mut self, input: u64, output: u64, cache: u64) {
        self.input_tokens += input;
        self.output_tokens += output;
        self.cache_tokens += cache;
    }

    pub fn save_cost(&mut self, cost: f64) {
        self.curr_balance -= cost;
    }

    pub fn update(&mut self, credits: &Credits) {
        self.curr_balance = self.curr_balance.min(credits.remaining);
    }

    pub fn delta_string(&self) -> String {
        let delta = self.start_balance - self.curr_balance;
        format!("${:.5}", delta)
    }

    pub fn remaining_string(&self) -> String {
        format!("${:.5}", self.curr_balance)
    }
}

impl From<&Credits> for Session {
    fn from(value: &Credits) -> Self {
        Self::new(value.remaining)
    }
}

pub struct Credits {
    pub balance: f64,
    pub usage: f64,
    pub remaining: f64,
}

impl Credits {
    pub fn new(balance: f64, usage: f64, remaining: f64) -> Self {
        Self {
            balance,
            usage,
            remaining,
        }
    }
}

pub fn get_credits(url: &str) -> anyhow::Result<Credits> {
    let client = reqwest::blocking::Client::new();
    let response: CreditsResponse = client.get(url).send()?.json()?;
    let remaining_balance = response.data.total_credits - response.data.total_usage;

    Ok(Credits::new(
        response.data.total_credits,
        response.data.total_usage,
        remaining_balance,
    ))
}
