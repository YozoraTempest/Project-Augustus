// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::*;

#[test]
fn minimum_clamp_preserves_inputs_and_round_trips() {
    for (input, expected, adjustment) in [(-500_000, 0, 500_000), (250_000, 250_000, 0)] {
        let mut world = World::new(Schema::new(["unused"]).unwrap());
        let country = world.add_entity("AAA", EntityKind::Country);
        let term = AttributeTerm::ClampMinimum {
            source: "decay_floor".into(),
            minimum_micros: 0,
            term: Box::new(AttributeTerm::Constant {
                source: "decay_input".into(),
                micros: input,
            }),
        };
        world
            .set_attributes(Attributes {
                formulas: [(
                    country,
                    [(
                        "annual_change".into(),
                        AttributeFormula {
                            base: AttributeBase::Constant { micros: 0 },
                            channels: vec![AttributeChannel {
                                flat: vec![term],
                                rates: vec![],
                            }],
                            minimum_micros: None,
                            maximum_micros: None,
                            result_quantum_micros: None,
                            unresolved: Default::default(),
                        },
                    )]
                    .into(),
                )]
                .into(),
                ..Default::default()
            })
            .unwrap();

        let evaluation = world
            .attributes()
            .evaluate(&world, country, "annual_change")
            .unwrap();
        assert_eq!(evaluation.value_micros, Some(expected));
        assert_eq!(
            evaluation.steps[0]
                .contributions
                .iter()
                .filter(|contribution| contribution.property == "minimum_clamp")
                .map(|contribution| contribution.micros)
                .sum::<i64>(),
            adjustment
        );

        let snapshot = world.save("minimum-clamp").unwrap();
        let restored = World::restore(&snapshot, "minimum-clamp", world.schema()).unwrap();
        assert_eq!(
            restored
                .attributes()
                .evaluate(&restored, country, "annual_change")
                .unwrap(),
            evaluation
        );
    }
}
