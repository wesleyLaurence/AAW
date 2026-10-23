import AAWCore
import XCTest
@testable import AAWApp

/// Plugins, devices of the person's own, in the app: the line a panel says
/// of one, and how the browser matches one to a search.
final class PluginTests: XCTestCase {
    private func state(_ state: String, error: String? = nil) -> DeviceStatusView {
        DeviceStatusView(effect: 7, reference: "tracks.bass.effects.tilt", device: "tilt", state: state, error: error)
    }

    func testThePanelSaysWhatTheHostIsDoingAndNothingWhileTheCachePlays() {
        XCTAssertNil(SongModel.deviceNote(nil))
        XCTAssertNil(SongModel.deviceNote(state("ready")))
        XCTAssertNil(SongModel.deviceNote(state("off")))
        XCTAssertTrue(SongModel.deviceNote(state("computing"))!.hasPrefix("Computing"))
        XCTAssertTrue(SongModel.deviceNote(state("waiting"))!.hasPrefix("Waiting"))
        XCTAssertEqual(SongModel.deviceNote(state("failed", error: "process raised")), "Failed: process raised; the sound passes through")
        XCTAssertTrue(SongModel.deviceNote(state("missing", error: "No device tilt"))!.hasPrefix("Not installed: No device tilt"))
        // An instrument's notes play nothing meanwhile; it passes nothing through.
        XCTAssertEqual(SongModel.deviceNote(state("failed", error: "play raised"), instrument: true), "Failed: play raised; the notes play nothing")
        XCTAssertTrue(SongModel.deviceNote(state("computing"), instrument: true)!.contains("they play nothing until it is done"))
    }

    func testAPluginIsMatchedByNameSlugDescriptionOrField() {
        let tilt = DeviceInfo(name: "Tilt EQ", slug: "tilt-eq", description: "A tilt about a pivot", version: "0.1", kind: "effect", fields: ["tilt_db", "pivot_hz"], from: "workspace", folder: "/w/devices/tilt-eq")
        XCTAssertTrue(Browser.matches(tilt, ""))
        XCTAssertTrue(Browser.matches(tilt, "tilt eq"))
        XCTAssertTrue(Browser.matches(tilt, "pivot"))
        XCTAssertTrue(Browser.matches(tilt, "TILT-EQ"))
        XCTAssertFalse(Browser.matches(tilt, "reverb"))
    }
}
