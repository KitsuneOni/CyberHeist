# How the end screens word a run, shared by the victory screen and the caught
# screen so both ends of a run report it the same way. Plain text only, with
# no autoloads, so tests can check the wording on its own.
extends RefCounted


# The run so far, from FlowCoordinator.run_summary().
static func summary_text(summary: Dictionary) -> String:
	return (
		"Zones cleared: %d of %d  ·  Fights won: %d  ·  Cards added: %d\nCredits earned this run: %d"
		% [
			int(summary.get("zones_cleared", 0)),
			int(summary.get("total_zones", 0)),
			int(summary.get("combats_won", 0)),
			int(summary.get("cards_added", 0)),
			int(summary.get("credits_earned", 0)),
		]
	)


# The contract line under each end screen's title, from
# FlowCoordinator.accepted_contract_offer().
static func contract_text(offer: Dictionary) -> String:
	if offer.get("ok", false):
		return "%s  ·  target: %s" % [offer.get("name", "?"), offer.get("target", "?")]
	return "Default contract"
