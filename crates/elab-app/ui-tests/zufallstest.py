#!/usr/bin/env python3
"""Zufallstest der Oberfläche: erzeugt ein Testskript mit vielen schnellen Aktionen
(Drag & Drop aus beiden Paletten, Klicks, Drähte, Drehen, Löschen, Undo/Redo, Taster,
Messgeräte-Reiter). Hat 2026-11 einen Absturz beim Platzieren mehrerer Bauteile gefunden.

    python3 crates/elab-app/ui-tests/zufallstest.py 1 140 > /tmp/z.txt
    cargo run --release -- --script /tmp/z.txt     # darf nicht abstürzen

Die Palettenkoordinaten gelten für die Standardfenstergröße 1560×980.
"""
import random, sys
seed=int(sys.argv[1]); n=int(sys.argv[2]); r=random.Random(seed)
dig=[(44,178),(120,178),(196,178),(272,178),(44,244),(120,244),(196,244),(44,325),(120,325),(196,325),(272,325),(44,391),(44,471),(120,471),(196,471),(44,551),(120,551),(196,551)]
ana=[(x,y) for y in (179,259) for x in (44,120,196,272)]+[(44,339),(120,339),(44,419),(120,419),(196,419)]
g=lambda: (r.randint(-8,22), r.randint(-8,12))
out=["wait 10","cmd N","wait 3"]
digital=False
for i in range(n):
    a=r.random()
    if a<0.08:
        digital=not digital; out.append("palette "+("digital" if digital else "analog"))
    elif a<0.40:
        t=r.choice(dig if digital else ana); x,y=g(); out.append(f"dragpx {t[0]} {t[1]} {x} {y}")
        if r.random()<0.5: out.append(f"move {x} {y}")
    elif a<0.52:
        x,y=g(); out.append(f"click {x} {y}")
    elif a<0.64:
        x,y=g(); x2,y2=g(); out.append(f"drag {x} {y} {x2} {y2}")
    elif a<0.70: out.append("key R")
    elif a<0.75: out.append("key Delete")
    elif a<0.80: out.append("cmd Z")
    elif a<0.83: out.append("cmdshift Z")
    elif a<0.87:
        x,y=g(); out.append(f"dblclick {x} {y}")
    elif a<0.90:
        x,y=g(); out.append(f"hold {x} {y}"); out.append("wait 2"); out.append("release")
    elif a<0.93: out.append("panel "+r.choice(["scope","analyzer","truth"]))
    elif a<0.96: out.append(f"advance {r.choice([0.1,0.5,2])}")
    else:
        x,y=g(); out.append(f"move {x} {y}"); out.append("wait 2")
out.append("wait 5"); out.append("quit")
print("\n".join(out))
