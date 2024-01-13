use crate::{entities::pool::Pool, error::Error};
use uniswap_sdk_core::entities::{
    currency::CurrencyTrait,
    fractions::{
        fraction::{FractionBase, FractionTrait},
        price::Price,
    },
    token::Token,
};

pub struct Route<TInput, TOutput>
where
    TInput: CurrencyTrait,
    TOutput: CurrencyTrait,
{
    pools: Vec<Pool>,
    token_path: Vec<Token>,
    input: TInput,
    output: TOutput,
    mid_price: Option<Price<TInput, TOutput>>,
}

impl<TInput, TOutput> Route<TInput, TOutput>
where
    TInput: CurrencyTrait,
    TOutput: CurrencyTrait,
{
    /// Construct a Route
    ///
    /// # Arguments
    ///
    /// * `pools`: array of pools
    /// * `inpit`: The other token in the pool
    /// * `output`: The fee in hundredths of a bips of the input amount of every swap that is collected by the pool
    pub fn new(pools: Vec<Pool>, input: TInput, output: TOutput) -> Result<Self, Error> {
        if pools.len() == 0 {
            return Err(Error::IsZero);
        }

        let chain_id = pools[0].chain_id();
        let all_on_some_chain = pools.iter().all(|pool| pool.chain_id() == chain_id);
        if !all_on_some_chain {
            return Err(Error::ChainIdIsDifferent);
        }

        let wrapped_input = input.wrapped().clone();
        if !(pools[pools.len() - 1].involves_token(&output.wrapped())) {
            return Err(Error::InvolvesToken);
        }

        let mut token_path = vec![wrapped_input];

        for (i, pool) in pools.iter().enumerate() {
            let current_input_token = &token_path[i];
            if !(current_input_token.equals(&pool.token0)
                || current_input_token.equals(&pool.token1))
            {
                return Err(Error::TokenNotInPool);
            }
            let next_token = if current_input_token.equals(&pool.token0) {
                &pool.token1
            } else {
                &pool.token0
            };
            token_path.push(next_token.clone());
        }

        Ok(Self {
            pools: pools,
            token_path: token_path,
            input: input,
            output: output,
            mid_price: None,
        })
    }

    pub fn chain_id(&self) -> u32 {
        self.pools[0].chain_id()
    }

    pub fn mid_price(&mut self) -> Price<TInput, TOutput> {
        if self.mid_price.is_none() {
            let token0_price = self.pools[0].token0_price().clone();
            let token1_price = self.pools[0].token1_price().clone();
            let initial_price = if self.pools[0].token0 == self.input.wrapped() {
                token1_price
            } else {
                token0_price
            };
            let price = self
                .pools
                .iter()
                .skip(1)
                .fold(
                    (
                        if self.pools[0].token0 == self.input.wrapped() {
                            &self.pools[0].token1
                        } else {
                            &self.pools[0].token0
                        },
                        initial_price,
                    ),
                    |(next_input, price), pool| {
                        if next_input == &pool.token0 {
                            (
                                &pool.token1,
                                price
                                    .multiply(&pool.clone().token0_price())
                                    .expect("Failed to multiply prices"),
                            )
                        } else {
                            (
                                &pool.token0,
                                price
                                    .multiply(&pool.clone().token1_price())
                                    .expect("Failed to multiply Prices"),
                            )
                        }
                    },
                )
                .1;

            Price::new(
                self.input.clone(),
                self.output.clone(),
                price.denominator().clone(),
                price.numerator().clone(),
            )
        } else {
            self.mid_price.clone().unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        constants::FeeAmount,
        entities::{pool::Pool, route::Route, Tick, TickListDataProvider},
        prelude::{encode_sqrt_ratio_x96, nearest_usable_tick},
    };
    use uniswap_sdk_core::{
        entities::{
            ether::Ether,
            token::Token,
            weth9::{self, WETH9}, fractions::fraction::FractionBase,
        },
        prelude::RoundingMode,
        token, constants::Rounding,
    };
    use uniswap_v3_math::tick_math;

    #[test]
    fn test_route() {
        let eth = Ether::on_chain(1);
        let token0 = token!(1, "0x0000000000000000000000000000000000000001", 18, "t0");
        let token1 = token!(1, "0x0000000000000000000000000000000000000002", 18, "t1");

        let weth_instance = WETH9::new();
        let weth = weth_instance.get(1).unwrap();

        let pool_0_1 = Pool::new(
            token0.clone(),
            token1.clone(),
            FeeAmount::MEDIUM,
            encode_sqrt_ratio_x96(1, 1),
            0,
            None,
        )
        .unwrap();
        let pool_0_weth = Pool::new(
            token0.clone(),
            weth.clone(),
            FeeAmount::MEDIUM,
            encode_sqrt_ratio_x96(1, 1),
            0,
            None,
        )
        .unwrap();
        let pool_1_weth = Pool::new(
            token1.clone(),
            weth.clone(),
            FeeAmount::MEDIUM,
            encode_sqrt_ratio_x96(1, 1),
            0,
            None,
        )
        .unwrap();
        let route_0_1 = Route::new(vec![pool_0_1.clone()], token0.clone(), token1.clone()).unwrap();

        //IT CONSTRUCTS A PATH FROM THE TOKEN
        assert_eq!(
            route_0_1.pools,
            vec![pool_0_1.clone()],
            "route_0_1.pools should be equal to pool_0_1"
        );
        assert_eq!(
            route_0_1.token_path,
            vec![token0.clone(), token1.clone()],
            "route.0_1.token_path should be equal to an vec![token0, token1]"
        );
        assert_eq!(
            route_0_1.input,
            token0.clone(),
            "roue_0_1.input should be equal to token0"
        );
        assert_eq!(
            route_0_1.output,
            token1.clone(),
            "route_0_output should be equal to token1"
        );
        assert_eq!(
            route_0_1.chain_id(),
            1,
            "route_0_1.chain_id should be equal to 1"
        );

        assert!(
            (Route::new(vec![pool_0_1.clone()], weth.clone(), token1.clone())).is_err(),
            "should fail if token is not present in the first pool"
        );

        assert!(
            (Route::new(vec![pool_0_1.clone()], token0.clone(), weth.clone())).is_err(),
            "should fail if token is not present in the first pool"
        );

        //CAN HAVE A TOKEN HAS BOTH INPUT AND OUTPUT
        let route_0_2 = Route::new(
            vec![pool_0_weth.clone(), pool_0_1.clone(), pool_1_weth.clone()],
            weth.clone(),
            weth.clone(),
        )
        .unwrap();

        assert_eq!(
            route_0_2.pools,
            vec![pool_0_weth.clone(), pool_0_1.clone(), pool_1_weth.clone()],
            "route_0_1.pools should be equal to pool_0_1"
        );

        assert_eq!(
            route_0_2.input,
            weth.clone(),
            "token in input  should be equal to weth"
        );

        assert_eq!(
            route_0_2.output,
            weth.clone(),
            "token in output should be equal to weth"
        );

        //IT SUPPORTS ETHER INPUT
        let route_0_3 = Route::new(vec![pool_0_weth.clone()], eth.clone(), token0.clone()).unwrap();

        assert_eq!(
            route_0_3.pools,
            vec![pool_0_weth.clone()],
            "route should be equal to pool_0_1"
        );

        assert_eq!(
            route_0_3.input,
            eth.clone(),
            "token in input should be equal to weth"
        );

        assert_eq!(
            route_0_3.output,
            token0.clone(),
            "token in output should be equal to weth"
        );

        //IT SUPPORTS ETHER OUTPUT
        let route_0_4 = Route::new(vec![pool_0_weth.clone()], token0.clone(), eth.clone()).unwrap();

        assert_eq!(
            route_0_4.pools,
            vec![pool_0_weth.clone()],
            "route should be equal to pool_0_1"
        );

        assert_eq!(
            route_0_4.input,
            token0.clone(),
            "token in input should be equal to weth"
        );

        assert_eq!(
            route_0_4.output,
            eth.clone(),
            "token in output should be equal to weth"
        );
    }

    #[test]
    fn test_mid_price() {
        let eth = Ether::on_chain(1);
        let token0 = token!(1, "0x0000000000000000000000000000000000000001", 18, "t0");
        let token1 = token!(1, "0x0000000000000000000000000000000000000002", 18, "t1");
        let token2 = token!(1, "0x0000000000000000000000000000000000000003", 18, "t2");

        let weth_instance = WETH9::new();
        let weth = weth_instance.get(1).unwrap();

        let pool_0_1 = Pool::new(
            token0.clone(),
            token1.clone(),
            FeeAmount::MEDIUM,
            encode_sqrt_ratio_x96(1, 5),
            0,
            None,
        )
        .unwrap();
        let pool_1_2 = Pool::new(
            token1.clone(),
            token2.clone(),
            FeeAmount::MEDIUM,
            encode_sqrt_ratio_x96(15, 30),
            0,
            Some(Arc::new(TickListDataProvider::new(
                vec![Tick::new(
                    nearest_usable_tick(tick_math::MIN_TICK, FeeAmount::LOW.tick_spacing()),
                    1,
                    5,
                )],
                FeeAmount::LOW.tick_spacing(),
            ))),
        )
        .unwrap();
        let pool_0_weth = Pool::new(
            token0.clone(),
            weth.clone(),
            FeeAmount::MEDIUM,
            encode_sqrt_ratio_x96(1, 1),
            0,
            Some(Arc::new(TickListDataProvider::new(
                vec![Tick::new(
                    nearest_usable_tick(tick_math::MIN_TICK, FeeAmount::LOW.tick_spacing()),
                    15,
                    30,
                )],
                FeeAmount::LOW.tick_spacing(),
            ))),
        )
        .unwrap();
        let pool_1_weth = Pool::new(
            token1.clone(),
            weth.clone(),
            FeeAmount::MEDIUM,
            encode_sqrt_ratio_x96(1, 1),
            0,
            Some(Arc::new(TickListDataProvider::new(
                vec![Tick::new(
                    nearest_usable_tick(tick_math::MIN_TICK, FeeAmount::LOW.tick_spacing()),
                    1,
                    7,
                )],
                FeeAmount::LOW.tick_spacing(),
            ))),
        )
        .unwrap();

        //IT CORRECT FOR 0 -> 1
        let price = Route::new(vec![pool_0_1.clone()], token0.clone(), token1.clone())
            .unwrap()
            .mid_price
            .unwrap();
        assert_eq!(price.to_fixed(0, Rounding::RoundDown), "0.2000".to_string());
    }
}
