let assets_with_compliance = 0;
let count_paused = 0;
let max_requirements = 0;
let max_sender_conditions = 0;
let max_receiver_conditions = 0;
let max_condition_claims = 0;
let max_condition_issuers = 0;
let max_condition_issuer_claim_types = 0;

let paged = STORAGE.map_paged("ComplianceManager", "AssetCompliances");
paged.set_page_count(100);

fn condition_claims(condition) {
	let c_type = condition.condition_type;
	if c_type.IsPresent != () {
		return 1;
	}
	if c_type.IsAbsent != () {
		return 1;
	}
	if c_type.IsAnyOf != () {
		return c_type.IsAnyOf.len();
	}
	if c_type.IsNoneOf != () {
		return c_type.IsNoneOf.len();
	}
	if c_type.IsIdentity != () {
		return 1;
	}
}

do {
	let compliances = paged.next();
	for compliance in compliances {
		let total_conditions = 0;
		let total_claims = 0;
		let total_issuers = 0;
		if compliance.paused {
			count_paused += 1;
		}
		let requirements = compliance.requirements;
		if requirements.len() > max_requirements {
			max_requirements = requirements.len();
		}
		print(`---- Asset requirements: ${requirements.len()}`);
		for requirement in requirements {
			let conditions = requirement.sender_conditions;
			if conditions.len() > max_sender_conditions {
				max_sender_conditions = conditions.len();
			}
			total_conditions += conditions.len();
			for condition in conditions {
				let claims = condition_claims(condition);
				total_claims += claims;
				if claims > max_condition_claims {
					max_condition_claims = claims;
				}
				let issuers = condition.issuers;
				total_issuers += issuers.len();
				if issuers.len() > max_condition_issuers {
					max_condition_issuers = issuers.len();
				}
			}
			let conditions = requirement.receiver_conditions;
			if conditions.len() > max_receiver_conditions {
				max_receiver_conditions = conditions.len();
			}
			total_conditions += conditions.len();
			for condition in conditions {
				let claims = condition_claims(condition);
				total_claims += claims;
				if claims > max_condition_claims {
					max_condition_claims = claims;
				}
				let issuers = condition.issuers;
				total_issuers += issuers.len();
				if issuers.len() > max_condition_issuers {
					max_condition_issuers = issuers.len();
				}
			}
		}
		print(` -- total conditions: ${total_conditions}`);
		print(` -- total claims: ${total_claims}`);
		print(` -- total issuers: ${total_issuers}`);
	}
	assets_with_compliance += compliances.len();
} while paged.has_more;

print(`assets_with_compliance = ${assets_with_compliance}`);
print(`count_paused = ${count_paused}`);
print(`max_requirements = ${max_requirements}`);
print(`max_sender_conditions = ${max_sender_conditions}`);
print(`max_receiver_conditions = ${max_receiver_conditions}`);
print(`max_condition_claims = ${max_condition_claims}`);
