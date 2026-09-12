import json, urllib.request, time
B="http://127.0.0.1:8765"
def post(p,b=None):
    r=urllib.request.Request(B+p,data=(json.dumps(b) if b is not None else "{}").encode(),method="POST",headers={"Content-Type":"application/json"})
    return json.loads(urllib.request.urlopen(r,timeout=300).read())
def get(p): return json.loads(urllib.request.urlopen(B+p,timeout=30).read())
post("/load_organ",{"path":r"C:\Bronbestanden\JM-Orgue\Sample set homemade\Friesach_GrandOrgue\Friesach.organ"}); time.sleep(3)
info=get("/organ"); stops=[s for d in info["divisions"] for s in d["stops"]]
for s in stops:
    if s.get("drawn"): post(f"/stops/{s['id']}/toggle")
time.sleep(3); st=get("/status"); print("stil:", st["voice_count"], "stemmen, load", round(st["render_load"]*100), "% piek", round(st["render_peak"]*100), "%")
post(f"/stops/{stops[0]['id']}/toggle")
for n in (60,64,67): post(f"/notes/{n}/on")
time.sleep(3); st=get("/status"); print("1 register/3 noten:", st["voice_count"], "stemmen, load", round(st["render_load"]*100), "% piek", round(st["render_peak"]*100), "%")
for n in (60,64,67): post(f"/notes/{n}/off")
for s in stops[:8]:
    if not s.get("drawn"): post(f"/stops/{s['id']}/toggle")
for n in (48,52,55,60,64,67,72): post(f"/notes/{n}/on")
time.sleep(3); st=get("/status"); print("8 registers/7 noten:", st["voice_count"], "stemmen, load", round(st["render_load"]*100), "% piek", round(st["render_peak"]*100), "%")
for n in (48,52,55,60,64,67,72): post(f"/notes/{n}/off")
time.sleep(1)
print("buffer:", st.get("buffer_frames"), "host:", st.get("audio_host"), "device:", st.get("audio_device"))
