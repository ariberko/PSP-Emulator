//! Writes a folder of synthetic PSP discs.
//!
//! Useful for developing the XMB against a realistic library without owning or
//! copying any real game. Every file is a genuine ISO, CSO or PBP with a valid
//! PARAM.SFO and a decodable ICON0.PNG, so it exercises the same parsing path a
//! real dump does.
//!
//! ```sh
//! cargo run -p psp-metadata --features testkit --example make-fixtures -- /tmp/roms
//! ```
//!
//! Titles are invented placeholders — this generates fixtures, not copies of
//! anyone's software.

use std::path::PathBuf;

use psp_metadata::testkit::{cso_store_only, IsoBuilder, PbpBuilder, SfoBuilder};
use psp_metadata::testkit_png::{backdrop_art, cover_art};

struct Fixture {
    file: &'static str,
    title: &'static str,
    disc_id: &'static str,
    from: [u8; 3],
    to: [u8; 3],
    kind: Kind,
}

enum Kind {
    Iso,
    Cso,
    Pbp,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        file: "aurora-drift.iso",
        title: "Aurora Drift",
        disc_id: "DEMO00001",
        from: [72, 138, 200],
        to: [16, 42, 72],
        kind: Kind::Iso,
    },
    Fixture {
        file: "blade-cadence.cso",
        title: "Blade Cadence",
        disc_id: "DEMO00002",
        from: [178, 62, 84],
        to: [58, 16, 30],
        kind: Kind::Cso,
    },
    Fixture {
        file: "cosmic-rally.iso",
        title: "Cosmic Rally",
        disc_id: "DEMO00003",
        from: [214, 146, 48],
        to: [74, 42, 10],
        kind: Kind::Iso,
    },
    Fixture {
        file: "deep-field.cso",
        title: "Deep Field",
        disc_id: "DEMO00004",
        from: [46, 150, 132],
        to: [10, 46, 44],
        kind: Kind::Cso,
    },
    Fixture {
        file: "echo-runner.pbp",
        title: "Echo Runner",
        disc_id: "DEMO00005",
        from: [112, 82, 176],
        to: [36, 26, 62],
        kind: Kind::Pbp,
    },
    Fixture {
        file: "lantern-keep.iso",
        title: "Lantern Keep",
        disc_id: "DEMO00006",
        from: [206, 118, 66],
        to: [62, 30, 16],
        kind: Kind::Iso,
    },
    Fixture {
        file: "nimbus-tactics.cso",
        title: "Nimbus Tactics",
        disc_id: "DEMO00007",
        from: [88, 124, 196],
        to: [24, 36, 76],
        kind: Kind::Cso,
    },
];

fn main() -> std::io::Result<()> {
    let target: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "fixtures/roms".to_string())
        .into();
    std::fs::create_dir_all(&target)?;

    for fixture in FIXTURES {
        let sfo = SfoBuilder::new()
            .text(
                "CATEGORY",
                if matches!(fixture.kind, Kind::Pbp) {
                    "MG"
                } else {
                    "UG"
                },
            )
            .text("DISC_ID", fixture.disc_id)
            .text("DISC_VERSION", "1.00")
            .text("PSP_SYSTEM_VER", "6.60")
            .text("TITLE", fixture.title)
            .int("PARENTAL_LEVEL", 3)
            .build();

        let icon = cover_art(fixture.from, fixture.to);
        let backdrop = backdrop_art(fixture.from, fixture.to);

        let bytes = match fixture.kind {
            Kind::Iso => IsoBuilder::new()
                .volume_id(fixture.disc_id)
                .param_sfo(sfo)
                .icon0(icon)
                .build(),
            Kind::Cso => cso_store_only(
                &IsoBuilder::new()
                    .volume_id(fixture.disc_id)
                    .param_sfo(sfo)
                    .icon0(icon)
                    .build(),
                2048,
            ),
            Kind::Pbp => PbpBuilder::new()
                .param_sfo(sfo)
                .icon0(icon)
                .pic1(backdrop)
                .data_psp(b"fixture payload, not a real executable".to_vec())
                .build(),
        };

        let path = target.join(fixture.file);
        std::fs::write(&path, &bytes)?;
        println!("{:>10} bytes  {}", bytes.len(), path.display());
    }

    // A save-data PBP, to prove the scanner filters CATEGORY=MS out of the list.
    let save = PbpBuilder::new()
        .param_sfo(
            SfoBuilder::new()
                .text("CATEGORY", "MS")
                .text("TITLE", "Aurora Drift - Slot 1")
                .build(),
        )
        .build();
    let save_path = target.join("aurora-drift-save.pbp");
    std::fs::write(&save_path, &save)?;
    println!(
        "{:>10} bytes  {} (save data, should be filtered out)",
        save.len(),
        save_path.display()
    );

    println!(
        "\nWrote {} fixtures to {}",
        FIXTURES.len() + 1,
        target.display()
    );
    Ok(())
}
