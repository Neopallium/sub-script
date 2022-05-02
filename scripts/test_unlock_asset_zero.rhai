import "mod/polymesh" as p;

print(`Create some investors.`);
let investors = p::create_users_batch("Investor", 0, 2, 1000.0);
let portfolios = [];
let dids = p::get_dids(investors);
for did in dids {
	portfolios.push(#{ did: did, kind: #{ Default: () } });
}

// Create an assets
let supply = 10000.0;
let tickers = ["ASSET1", "ASSET2", "ASSET3"];
let res = p::create_assets(tickers, #{ EquityCommon: () }, supply * (investors.len() + 10), true);
let assets_issuer = res[0];
let assets = res[1];

// Create one user for the "Exchange".
let exchanges = p::create_users_batch("Exchange", 0, 1, 100000.0);
let exchange = exchanges[0];

print(`Give some of the investors some assets`);
for (asset, idx) in assets {
	let issuer = assets_issuer[idx];
	p::asset_fund_users(asset, issuer, [investors[0]], supply);
}

// Create venue.
let venue_id = p::get_or_create_venue(exchange, #{ Other: () });

// Setup instruction legs.
let legs = [
	#{
		from: portfolios[0],
		to: portfolios[1],
		asset: assets[0],
		amount: 10.0,
	},
	#{
		from: portfolios[1],
		to: portfolios[0],
		asset: assets[1],
		amount: 0.0,
	},
];

// Add instruction.
print("Create final Settlement.");
let res = exchange.submit(Settlement.add_instruction(venue_id, #{ SettleOnAffirmation: () }, (), (), legs));
print(`res: ${res.is_success}`);
let instruction_id = ();
for event in res.events("Settlement.InstructionCreated") {
	instruction_id = event.args[2];
}

// affirm each investor's legs.
print("Affirm the legs.");
let results = [];
let max_legs = legs.len();
/*
for (portfolio, idx) in portfolios {
	let user = investors[idx];
	results += user.submit(Settlement.affirm_instruction(instruction_id, [portfolio], max_legs));
}
*/
// Only affirm one side.
let portfolio = portfolios[1];
let user = investors[1];
results += user.submit(Settlement.affirm_instruction(instruction_id, [portfolio], max_legs));
p::wait_for_in_block(results);

print("Finished");
