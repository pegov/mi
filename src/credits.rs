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
}

impl Session {
    fn new(start_balance: f64) -> Self {
        Self {
            start_balance,
            curr_balance: start_balance,
            input_tokens: 0,
            output_tokens: 0,
            cache_tokens: 0,
        }
    }

    pub fn save_tokens(&mut self, input: u64, output: u64, cache: u64) {
        self.input_tokens = self.input_tokens.saturating_add(input);
        self.output_tokens = self.output_tokens.saturating_add(output);
        self.cache_tokens = self.cache_tokens.saturating_add(cache);
    }

    pub fn update(&mut self, credits: &Credits) {
        self.curr_balance = credits.remaining;
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
