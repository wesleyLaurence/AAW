import AAWCore
import XCTest
@testable import AAWApp

/// Favorites in the browser: which group each is listed under, how a row
/// is matched to the file, and the row for a sample the index lacks.
final class BrowserFavoritesTests: XCTestCase {
    private func favorite(_ kind: String, _ what: String, slug: String = "", missing: Bool = false) -> FavoriteInfo {
        FavoriteInfo(kind: kind, name: what, what: what, slug: slug, missing: missing)
    }

    func testEachFavoriteIsListedUnderItsSectionsGroup() {
        XCTAssertEqual(Browser.favoriteGroup(favorite("sample", "/a/kick.wav")), "Samples")
        XCTAssertEqual(Browser.favoriteGroup(favorite("patch", "Soft Pad", slug: "soft-pad")), "Instruments")
        XCTAssertEqual(Browser.favoriteGroup(favorite("instrument_rack", "Warm Bass", slug: "warm-bass")), "Instruments")
        XCTAssertEqual(Browser.favoriteGroup(favorite("device", "synth")), "Instruments")
        XCTAssertEqual(Browser.favoriteGroup(favorite("device", "sampler")), "Instruments")
        XCTAssertEqual(Browser.favoriteGroup(favorite("device", "arpeggiator")), "MIDI Effects")
        XCTAssertEqual(Browser.favoriteGroup(favorite("device", "compressor")), "Audio Effects")
        XCTAssertEqual(Browser.favoriteGroup(favorite("rack", "Drum Glue", slug: "drum-glue")), "Audio Effects")
    }

    func testARowIsMatchedByPathKindOrSlug() {
        let all = [favorite("sample", "/a/kick.wav"), favorite("patch", "Soft Pad", slug: "soft-pad"), favorite("device", "reverb")]
        XCTAssertNotNil(Browser.favorite(in: all, kind: "sample", "/a/kick.wav"))
        XCTAssertNil(Browser.favorite(in: all, kind: "sample", "/a/snare.wav"))
        XCTAssertNotNil(Browser.favorite(in: all, kind: "patch", "soft-pad"))
        XCTAssertNil(Browser.favorite(in: all, kind: "patch", "Soft Pad"), "a patch is matched by its slug, not its name")
        XCTAssertNotNil(Browser.favorite(in: all, kind: "device", "reverb"))
        XCTAssertNil(Browser.favorite(in: all, kind: "rack", "reverb"))
    }

    func testASampleTheIndexLacksIsShownFromItsPath() {
        let row = Browser.standIn(for: favorite("sample", "/Users/me/Samples/Pack One/kick 01.wav"))
        XCTAssertEqual(row.name, "kick 01.wav")
        XCTAssertEqual(row.pack, "Pack One")
        XCTAssertEqual(row.path, "/Users/me/Samples/Pack One/kick 01.wav")
        XCTAssertEqual(row.id, row.path)
        XCTAssertNil(row.rootNote)
        XCTAssertEqual(Browser.detail(of: row), "0.00 s")
        XCTAssertEqual(Browser.padName(of: row), "kick 01")
    }

    func testFavoritesComeFirstAmongTheSections() {
        XCTAssertEqual(Browser.sections.first, "Favorites")
        XCTAssertEqual(Browser.sections, ["Favorites", "Samples", "Generate", "Instruments", "MIDI Effects", "Audio Effects", "Folders"])
    }
}
