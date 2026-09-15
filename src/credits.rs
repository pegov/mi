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
}

impl Session {
    fn new(start_balance: f64) -> Self {
        Self {
            start_balance,
            curr_balance: start_balance,
        }
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
