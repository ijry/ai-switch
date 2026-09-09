use crate::error::AppError;
use crate::saas::repository::{invalid, money, MAX_MONEY};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BillableUsage {
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub output_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelPrice {
    pub input_price_micros: i64,
    pub cache_price_micros: i64,
    pub output_price_micros: i64,
    pub multiplier_micros: i64,
}

impl ModelPrice {
    pub fn charge(&self, usage: &BillableUsage) -> Result<i64, AppError> {
        if [
            self.input_price_micros,
            self.cache_price_micros,
            self.output_price_micros,
        ]
        .iter()
        .any(|price| !(0..=MAX_MONEY).contains(price))
            || !(1..=1_000_000_000).contains(&self.multiplier_micros)
            || [
                usage.input_tokens,
                usage.cache_read_tokens,
                usage.cache_write_tokens,
                usage.output_tokens,
            ]
            .iter()
            .any(|tokens| *tokens < 0)
        {
            return Err(invalid(
                "saas.pricing",
                "Invalid price, multiplier or token usage",
            ));
        }
        let input = (usage.input_tokens as i128 + usage.cache_write_tokens as i128)
            .checked_mul(self.input_price_micros as i128);
        let cache = (usage.cache_read_tokens as i128).checked_mul(self.cache_price_micros as i128);
        let output = (usage.output_tokens as i128).checked_mul(self.output_price_micros as i128);
        let weighted = input
            .and_then(|input| cache.and_then(|cache| input.checked_add(cache)))
            .and_then(|sum| output.and_then(|output| sum.checked_add(output)))
            .and_then(|sum| sum.checked_mul(self.multiplier_micros as i128))
            .ok_or_else(|| invalid("saas.amount_overflow", "Token charge overflow"))?;
        let denominator = 1_000_000_000_000_i128;
        money(weighted / denominator + i128::from(weighted % denominator != 0))
    }
}

pub fn recharge_credit(amount_cny_fen: i64, exchange_rate_micros: i64) -> Result<i64, AppError> {
    if !(1..=1_000_000_000).contains(&amount_cny_fen)
        || !(1..=MAX_MONEY).contains(&exchange_rate_micros)
    {
        return Err(invalid(
            "saas.recharge_amount",
            "Invalid recharge amount or exchange rate",
        ));
    }
    let credit =
        money(amount_cny_fen as i128 * 10_000_000_000_i128 / exchange_rate_micros as i128)?;
    if credit == 0 {
        return Err(invalid(
            "saas.recharge_amount",
            "Recharge is below one microdollar",
        ));
    }
    Ok(credit)
}
