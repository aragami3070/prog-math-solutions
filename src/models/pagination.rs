use serde::Deserialize;
use utoipa::IntoParams;

#[derive(Debug, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct Pagination {
    #[serde(default = "first_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn first_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    20
}

impl Pagination {
    pub fn valid(&self) -> bool {
        (1..=1_000_000).contains(&self.page) && (1..=100).contains(&self.page_size)
    }

    pub fn offset(&self) -> i64 {
        i64::from(self.page - 1) * i64::from(self.page_size)
    }
}
