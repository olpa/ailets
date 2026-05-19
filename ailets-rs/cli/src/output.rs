//! Output subsystem - OutputSink trait, color support
//!
//! This module provides two related but separate abstractions:
//!
//! 1. **`OutputSink` trait** - High-level text output interface
//!    - Simple `println(&str)` method for line-oriented output
//!    - Easy to implement for testing (see `CapturingSink` in tests)
//!    - Used for command output and notifications
//!
//! 2. **`OutputSinkWriter` struct** - Adapter for byte-stream APIs
//!    - Wraps an `OutputSink` to implement `std::io::Write`
//!    - Needed when interfacing with APIs that require `Write` trait
//!    - Adds optional ANSI color support
//!
//! The separation exists to keep `OutputSink` simple and testable while
//! still allowing integration with `std::io::Write`-based APIs.

use std::sync::Arc;

// ---------------------------------------------------------------------------
// OutputSink
// ---------------------------------------------------------------------------

/// Where DagShell output is written. `Send + Sync` so the notification
/// watcher thread can hold an `Arc<dyn OutputSink>`.
///
/// This trait is intentionally simple to make it easy to implement custom
/// sinks for testing and alternative output destinations.
pub trait OutputSink: Send + Sync {
    fn print(&self, text: &str);

    fn println(&self, line: &str) {
        self.print(line);
        self.print("\n");
    }
}

pub struct StdoutSink;

impl OutputSink for StdoutSink {
    fn print(&self, text: &str) {
        print!("{text}");
    }
}

// ---------------------------------------------------------------------------
// Color support
// ---------------------------------------------------------------------------

pub fn parse_color(s: &str) -> Result<u8, String> {
    if let Ok(n) = s.parse::<u8>() {
        return Ok(n);
    }
    let key = s.to_ascii_lowercase().replace(['-', '_', ' '], "");
    named_color(&key).ok_or_else(|| format!("unknown color '{s}'; use a CSS/X11 name or 0-255"))
}

#[allow(clippy::too_many_lines)]
fn named_color(name: &str) -> Option<u8> {
    Some(match name {
        // Standard 16 terminal colors
        "black"                             =>   0,
        "maroon"                            =>   1,
        "darkgreen"                         =>   2,
        "olive" | "darkyellow"              =>   3,
        "navy"                              =>   4,
        "purple" | "darkmagenta"            =>   5,
        "teal" | "darkcyan"                 =>   6,
        "silver" | "lightgray" | "lightgrey"=>   7,
        "darkgray" | "darkgrey"
        | "grey" | "gray"                   =>   8,
        "red"                               =>   9,
        "green" | "lime"                    =>  10,
        "yellow"                            =>  11,
        "blue"                              =>  12,
        "fuchsia" | "magenta"               =>  13,
        "aqua" | "cyan"                     =>  14,
        "white"                             =>  15,
        // 256-color extended names
        "darkred"                           =>  88,
        "darkblue"                          =>  18,
        "deepskyblue"                       =>  39,
        "dodgerblue"                        =>  33,
        "royalblue"                         =>  62,
        "steelblue"                         =>  67,
        "cornflowerblue"                    =>  69,
        "skyblue"                           => 117,
        "lightskyblue"                      => 117,
        "lightblue"                         => 152,
        "powderblue"                        => 153,
        "lightsteelblue"                    => 147,
        "cadetblue"                         =>  73,
        "mediumblue"                        =>  20,
        "midnightblue"                      =>  18,
        "indigo"                            =>  54,
        "darkslateblue"                     =>  60,
        "slateblue"                         =>  62,
        "mediumslateblue"                   => 105,
        "mediumpurple"                      => 141,
        "blueviolet"                        =>  57,
        "darkviolet"                        =>  92,
        "darkorchid"                        =>  98,
        "orchid"                            => 170,
        "violet"                            => 213,
        "plum"                              => 183,
        "lavender"                          => 189,
        "thistle"                           => 182,
        "mediumorchid"                      => 134,
        "darkmagentaext"                    =>  90,
        "mediumvioletred"                   => 162,
        "palevioletred"                     => 168,
        "hotpink"                           => 205,
        "deeppink"                          => 197,
        "pink"                              => 218,
        "lightpink"                         => 217,
        "crimson"                           => 160,
        "firebrick"                         => 124,
        "darkred2"                          =>  52,
        "indianred"                         => 131,
        "lightcoral"                        => 210,
        "salmon"                            => 209,
        "darksalmon"                        => 173,
        "lightsalmon"                       => 216,
        "tomato"                            => 202,
        "orangered"                         => 202,
        "darkorange"                        => 208,
        "orange"                            => 214,
        "coral"                             => 209,
        "gold"                              => 220,
        "goldenrod"                         => 178,
        "darkgoldenrod"                     => 136,
        "yellow2"                           => 226,
        "lightyellow"                       => 230,
        "lemonchiffon"                      => 230,
        "khaki"                             => 185,
        "darkkhaki"                         => 143,
        "palegoldenrod"                     => 229,
        "chartreuse"                        => 118,
        "lawngreen"                         => 118,
        "greenyellow"                       => 154,
        "yellowgreen"                       => 148,
        "limegreen"                         =>  40,
        "mediumspringgreen"                 =>  48,
        "springgreen"                       =>  48,
        "green2"                            =>  46,
        "forestgreen"                       =>  28,
        "seagreen"                          =>  29,
        "mediumseagreen"                    =>  35,
        "darkseagreen"                      => 108,
        "palegreen"                         => 120,
        "lightgreen"                        => 120,
        "darkolivegreen"                    =>  58,
        "olivedrab"                         =>  64,
        "darkturquoise"                     =>  44,
        "mediumturquoise"                   =>  80,
        "turquoise"                         =>  80,
        "aquamarine"                        => 122,
        "mediumaquamarine"                  =>  79,
        "paleturquoise"                     => 159,
        "lightcyan"                         => 195,
        "lightseagreen"                     =>  37,
        "cyan2"                             =>  51,
        "rosybrown"                         => 138,
        "sienna"                            => 130,
        "saddlebrown"                       =>  94,
        "chocolate"                         => 166,
        "peru"                              => 136,
        "sandybrown"                        => 215,
        "tan"                               => 180,
        "burlywood"                         => 180,
        "wheat"                             => 229,
        "moccasin" | "peachpuff"            => 223,
        "navajowhite"                       => 223,
        "brown"                             => 124,
        "slategray" | "slategrey"           => 103,
        "lightslategray" | "lightslategrey" => 103,
        "darkslategray" | "darkslategrey"   =>  23,
        "dimgray" | "dimgrey"               => 241,
        "gainsboro"                         => 253,
        "whitesmoke"                        => 255,
        // Grayscale ramp (grey0-grey23 → indices 232-255)
        "grey0"  | "gray0"                  => 232,
        "grey1"  | "gray1"                  => 233,
        "grey2"  | "gray2"                  => 234,
        "grey3"  | "gray3"                  => 235,
        "grey4"  | "gray4"                  => 236,
        "grey5"  | "gray5"                  => 237,
        "grey6"  | "gray6"                  => 238,
        "grey7"  | "gray7"                  => 239,
        "grey8"  | "gray8"                  => 240,
        "grey9"  | "gray9"                  => 241,
        "grey10" | "gray10"                 => 242,
        "grey11" | "gray11"                 => 243,
        "grey12" | "gray12"                 => 244,
        "grey13" | "gray13"                 => 245,
        "grey14" | "gray14"                 => 246,
        "grey15" | "gray15"                 => 247,
        "grey16" | "gray16"                 => 248,
        "grey17" | "gray17"                 => 249,
        "grey18" | "gray18"                 => 250,
        "grey19" | "gray19"                 => 251,
        "grey20" | "gray20"                 => 252,
        "grey21" | "gray21"                 => 253,
        "grey22" | "gray22"                 => 254,
        "grey23" | "gray23"                 => 255,
        _                                   => return None,
    })
}

// ---------------------------------------------------------------------------
// OutputSinkWriter — adapts OutputSink as std::io::Write for attach_stdout_to
// ---------------------------------------------------------------------------

/// Adapter that wraps an `OutputSink` to implement `std::io::Write`.
///
/// This is needed when interfacing with APIs that require `std::io::Write`,
/// such as `attach_stdout_to()` which captures process output as a byte stream.
///
/// Writes bytes immediately (no buffering) and optionally colorizes output
/// with a 256-color ANSI code.
pub struct OutputSinkWriter {
    sink: Arc<dyn OutputSink>,
    color: Option<u8>,
}

impl OutputSinkWriter {
    pub fn new(sink: Arc<dyn OutputSink>, color: Option<u8>) -> Self {
        Self { sink, color }
    }
}

impl std::io::Write for OutputSinkWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let text = String::from_utf8_lossy(data);
        let output = match self.color {
            Some(c) => format!("\x1b[38;5;{c}m{text}\x1b[0m"),
            None    => text.into_owned(),
        };
        self.sink.print(&output);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
