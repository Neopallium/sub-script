import "polymesh" as p;

print("Create stablecoin");

// Get arguments.
let coin = ARG.shift();
if coin == () { coin = "TST" };
let balance = ARG.shift();
balance = if balance == () { 1000000.0 } else { parse_decimal(balance) };

print(`Create coin: ${coin}, initial supply: ${balance}`);

// Coin issuer.
let issuer = USER[`Issuer_${coin}`];
p::mock_user(issuer, 100000.0);

let res = issuer.submit(Utility.batch_atomic([
	Asset.create_asset(`Coin ${coin}`, coin, true, #{ StableCoin: () }, [], (), true),
	Asset.issue(coin, balance),
]));
print(`res: ${res}`);
