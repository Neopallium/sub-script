import "mod/polymesh" as p;

print("Create assets.");

// Get arguments.
let prefix = ARG.shift();
if prefix == () {
	print("Need ticker prefix");
	return;
}
let count = ARG.shift();
count = if count == () { 10 } else { parse_int(count) };
let supply = ARG.shift();
supply = if supply == () { 10.0 } else { parse_decimal(supply) };
let offset = ARG.shift();
offset = if offset == () { 0 } else { parse_int(offset) };
count += offset;

let res = p::create_asset_batch(prefix, offset, count, supply, true);
let issuers = res[0];
let tickers = res[1];

let tms = [
	#{ CountTransferManager: 1000 },
	#{ PercentageTransferManager: 400000 },
	#{ CountTransferManager: 1001 },
];
let results = [];
for (ticker, idx) in tickers {
	let issuer = issuers[idx];
	for tm in tms {
		results += issuer.submit(Statistics.add_transfer_manager(ticker, tm));
	}
}
p::wait_for_in_block(results);

