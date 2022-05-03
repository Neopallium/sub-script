
// Get arguments.
let start_block = ARG.shift();
if start_block == () {
	print("Need start block number");
	return;
}
start_block = parse_int(start_block);
// Number of blocks to search.
let count = ARG.shift();
count = if count == () { 10 } else { parse_int(count) };
// What to search for.
let filter = ARG.shift();
filter = if filter == () { "98002408011220a01a227d0b9f75830" } else { filter };

let end_block = start_block - count;
//let block_num = start_block;//2108133;
let block = CLIENT.get_block_by_number(start_block);

// Search backwards from `start_block`
let counter = 0;
while block.block_number > end_block {
	//print(`block = ${block}`);

	// Try to find ImOnline.Heartbeat calls.
	let calls = block.extrinsics_filtered(filter);
	if calls.len() > 0 {
		print(`${block.block_number}: calls = ${calls}`);
	} else {
		counter += 1;
		if counter > 1000 {
			counter = 0;
			print(`number = ${block.block_number}`);
		}
	}

	block = CLIENT.get_block(block.parent);
}

