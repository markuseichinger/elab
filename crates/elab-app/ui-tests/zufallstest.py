#!/usr/bin/env python3
"""Zufallstest der Oberfläche: erzeugt ein Testskript mit vielen schnellen Aktionen
(Drag & Drop aus allen drei Paletten, Klicks, Drähte, Drehen, Löschen, Undo/Redo, Taster,
Messgeräte-Reiter). Hat 2026-11 einen Absturz beim Platzieren mehrerer Bauteile gefunden.

    python3 crates/elab-app/ui-tests/zufallstest.py 1 140 > /tmp/z.txt
    cargo run --release -- --script /tmp/z.txt     # darf nicht abstürzen

Bauteile werden per Name aus der Palette gezogen (Befehl paldrag).
"""
import random, sys
seed=int(sys.argv[1]); n=int(sys.argv[2]); r=random.Random(seed)
parts=["Gleichspannung","Gleichspannung 5 V","Wechselspannung","Rechteckspannung","Stromquelle","Widerstand","Potentiometer","Kondensator",
       "Spule","Glühlampe","Diode","Leuchtdiode","NPN-Transistor","PNP-Transistor","N-Kanal-MOSFET","P-Kanal-MOSFET",
       "Schalter","Taster","Masse","Messpunkt","Voltmeter","Amperemeter",
       "UND-Gatter","ODER-Gatter","NICHT-Gatter","NAND-Gatter","NOR-Gatter","XOR-Gatter","XNOR-Gatter","Logikschalter",
       "Logiktaster","Taktgeber","Pegel 1","Logik-Anzeige","7-Segment-Anzeige","D-Flipflop","JK-Flipflop","4-Bit-Zähler",
       "NAND-Baustein","NOR-Baustein","UND-Baustein","ODER-Baustein","XOR-Baustein","NICHT-Baustein","Schmitt-Trigger",
       "D-Flipflop-IC","Zähler-IC","Operationsverstärker","Timer 555","A/D-Umsetzer","D/A-Umsetzer"]
g=lambda: (r.randint(-8,22), r.randint(-8,12))
out=["wait 10","cmd N","wait 3"]
for i in range(n):
    a=r.random()
    if a<0.36:
        x,y=g(); out.append(f"paldrag {r.choice(parts)} {x} {y}")
        if r.random()<0.5: out.append(f"move {x} {y}")
    elif a<0.48:
        x,y=g(); out.append(f"click {x} {y}")
    elif a<0.60:
        x,y=g(); x2,y2=g(); out.append(f"drag {x} {y} {x2} {y2}")
    elif a<0.65: out.append("key R")
    elif a<0.69: out.append("key Delete")
    elif a<0.74: out.append("cmd Z")
    elif a<0.77: out.append("cmdshift Z")
    elif a<0.81:
        x,y=g(); out.append(f"dblclick {x} {y}")
    elif a<0.85:
        x,y=g(); out.append(f"hold {x} {y}"); out.append("wait 2"); out.append("release")
    elif a<0.88: out.append("panel "+r.choice(["scope","analyzer","truth"]))
    elif a<0.91: out.append(f"advance {r.choice([0.1,0.5,2])}")
    elif a<0.94: out.append(r.choice(["key ArrowRight","key ArrowLeft"]))
    else:
        x,y=g(); out.append(f"move {x} {y}"); out.append("wait 2")
out.append("wait 5"); out.append("quit")
print("\n".join(out))
