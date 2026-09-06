# synth — synthetic input helper (macOS)

Build: `swiftc -O -o tools/synth/synth tools/synth/synth.swift` (a prebuilt
binary is checked in next to it; rebuild if it refuses to run).

    tools/synth/synth bounds <PID>            # prints: windowid x y w h (one line per visible window)
    tools/synth/synth click <X> <Y> <COUNT> [GAP_MS]   # CGEvent left-click COUNT times at screen point (double-click = 2 120)
    tools/synth/synth key <KEYCODE> [shift]   # key down/up; Return=36 Tab=48 Esc=53 Space=49 Delete=51 Left=123 Right=124 Down=125 Up=126
                                              # letters: a=0 s=1 d=2 f=3 h=4 g=5 z=6 x=7 c=8 v=9 b=11 q=12 w=13 e=14 r=15 y=16 t=17
                                              # digits: 1=18 2=19 3=20 4=21 6=22 5=23 =24 9=25 7=26 -=27 8=28 0=29 .=47 ,=43

Screenshot a window region: `screencapture -x -R <x>,<y>,<w>,<h> out.png`
(coordinates from `bounds`; the region excludes the shadow). Keyboard
shortcuts with modifiers: `osascript -e 'tell application "System Events" to keystroke "w" using command down'`.
The app must be frontmost for key events; click into it first.
