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

p::create_asset_batch(prefix, offset, count, supply, true);
