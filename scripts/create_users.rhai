import "mod/polymesh" as p;

print("Create users");

// Get arguments.
let prefix = ARG.shift();
if prefix == () {
	print("Need user prefix");
	return;
}
let count = ARG.shift();
count = if count == () { 10 } else { parse_int(count) };
let balance = ARG.shift();
balance = if balance == () { 10.0 } else { parse_decimal(balance) };
let offset = ARG.shift();
offset = if offset == () { 0 } else { parse_int(offset) };
count += offset;

p::create_users_batch(prefix, offset, count, balance);
