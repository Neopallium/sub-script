import "mod/polymesh" as p;

print(`Create some investors.`);
let investors = p::create_users_batch("Investor", 0, 2, 1000.0);

let coin_prefix = "COINS";
let coin_balance = 100000.0;
let asset_prefix = "ASSET";
let asset_balance = 1000.0;
let coin = "COINS0";
let asset = "ASSET0";
let asset_issuer = USER.Alice;

// Create one user for the "Exchange".
let exchanges = p::create_users_batch("Exchange", 0, 1, 100000.0);
let exchange = exchanges[0];

//*
let coin_supply = 10000.0 + (coin_balance * investors.len());
let res = p::create_coin_batch(coin_prefix, 0, 1, coin_supply);
let coin_issuer = res[0][0];
let coin = res[1][0];

print(`Give the investors ${coin_balance} x ${coin}`);
p::asset_fund_users(coin, coin_issuer, investors, coin_balance);

let asset_supply = 10000.0 + (asset_balance * investors.len());
let res = p::create_asset_batch(asset_prefix, 0, 1, asset_supply, true);
let asset_issuer = res[0][0];
let asset = res[1][0];

print(`Give the investors ${asset_balance} x ${asset}`);
p::asset_fund_users(asset, asset_issuer, investors, asset_balance);
// */

// Create Orderbook
let venue_id = p::get_or_create_venue(exchange, #{ OrderBook: () });
let res = exchange.submit(OrderBook.create_orderbook(venue_id, asset, #{ Asset: coin }));
let orderbook_id = ();
for event in res.events("OrderBook.OrderBookCreated") {
	orderbook_id = event.args[1];
}

// Make a list of traders(user, portfolio).
let traders = [];
for investor in investors {
	let did = p::get_did(investor);
	let portfolio = #{ did: did, kind: #{ Default: () } };
	traders.push(#{ investor: investor, portfolio: portfolio });
}

// Investors "deposit" funds for trading into the orderbook.
let results = [];
for trader in traders {
	// Deposit coin.
	results += trader.investor.submit(OrderBook.lock_asset_to_orderbook(orderbook_id, trader.portfolio, coin, coin_balance));
	// Deposit asset.
	results += trader.investor.submit(OrderBook.lock_asset_to_orderbook(orderbook_id, trader.portfolio, asset, asset_balance));
}
p::wait_for_in_block(results);

// Create some simple matched orders.
let trader = traders[0];
let buy = p::sign_orderbook_order(trader.investor, #{
	side: #{ Buy: () },
	order_book_id: orderbook_id,
	asset_portfolio: trader.portfolio,
	amount: 100.0,
	coin_portfolio: trader.portfolio,
	price: 10.0,
	fee: #{ OffChain: () },
	account_id: trader.investor.acc,
	order_id: 0,
	timestamp: 0,
});
let trader = traders[1];
let sell = p::sign_orderbook_order(trader.investor, #{
	side: #{ Sell: () },
	order_book_id: orderbook_id,
	asset_portfolio: trader.portfolio,
	amount: 100.0,
	coin_portfolio: trader.portfolio,
	price: 10.0,
	fee: #{ OffChain: () },
	account_id: trader.investor.acc,
	order_id: 0,
	timestamp: 0,
});

// Try to settle the matched orders.
let settle_orders = #{
	main_order: sell,
	matched_orders: [
		#{ order: buy, amount: 100.0 },
	]
};
let res = exchange.submit(OrderBook.settle_off_chain_orders(orderbook_id, settle_orders));
print(`res = ${res.result}`);
