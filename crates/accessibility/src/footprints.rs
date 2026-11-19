//! E12 ROW-to-E13 physical-space bridge. Row ownership is supplied by E12's
//! screened segment/ROW crosswalk, not one footprint per directed graph edge.
use crate::{
    scoring::{check, Context, Error},
    spatial::{road_space, Footprint, Inventory, SpaceScore},
};
use interventions::feasibility::Screened;
pub fn screened_road_space(
    gate: &Screened,
    context: &Context,
    inventory: &Inventory,
    footprints: &[Footprint],
) -> Result<SpaceScore, Error> {
    check(
        context.scenario == *gate.snapshot().id()
            && context.graph_revision == gate.snapshot().network().data().revision
            && context.graph_snapshot == gate.snapshot().network().encode(),
        "road-space snapshot mismatch",
    )?;
    let rows = gate.inputs().rows.iter().map(|r| r.id.clone()).collect();
    check(
        inventory.ids == rows,
        "footprint inventory must cover each screened physical ROW exactly once",
    )?;
    road_space(context, inventory, footprints)
}
