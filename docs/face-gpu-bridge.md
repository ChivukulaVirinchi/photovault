# Cloud GPU face acceleration (opt-in)

Smriti can offload face embedding to a free GPU notebook you own. Default
behaviour is unchanged — local ONNX Runtime stays primary. The bridge is
strictly additive and **never required**.

## What it sends

Only 112×112 aligned face JPEG crops (~5 KB each). No photos, no metadata,
no telemetry. You control the endpoint URL.

## Setup paths

### Kaggle (free, 30 hrs GPU/week, T4×2)

1. Open [kaggle.com](https://kaggle.com), create an account (free).
2. Create a new Notebook. Choose **GPU T4×2** as the accelerator.
3. Upload `notebooks/face_bridge.ipynb` from the Smriti repo.
4. Sign up for [ngrok](https://ngrok.com) (free). Copy your auth token from
   the dashboard.
5. Paste your ngrok token in the notebook's cell 5.
6. Run all cells. The last cell prints a public URL — copy it.

### Colab (free with limits / Pro $10/mo for A100)

1. Open [colab.research.google.com](https://colab.research.google.com).
2. File → Upload notebook → `notebooks/face_bridge.ipynb`.
3. Runtime → Change runtime type → **T4 GPU** (or **A100** on Pro).
4. Set up ngrok as above (or use the cloudflared alternative commented out).
5. Run all cells. Copy the printed URL.

### Local LAN GPU server (no internet)

1. Run `notebooks/face_bridge.ipynb` on a local machine with a GPU.
2. Skip the tunnel cells. The server binds to `0.0.0.0:8000`.
3. In Smriti Settings, enter `http://<lan-ip>:8000` as the bridge URL.

## How to connect Smriti

1. Open Smriti → Settings → **Cloud face acceleration (advanced)**.
2. Toggle "Use a remote GPU for face embedding" on.
3. Paste the bridge URL (e.g. `https://abc.ngrok.io`).
4. Click **Test connection** → should show "✓ CUDAExecutionProvider @ XXms".
5. Run face detection. Embedding goes remote; detection stays local.

## ngrok auth token (60-second setup)

1. Sign up at [ngrok.com](https://ngrok.com) (free, no credit card).
2. Copy your authtoken from [dashboard.ngrok.com/get-started/your-authtoken](https://dashboard.ngrok.com/get-started/your-authtoken).
3. Paste it in the notebook's `ngrok.set_auth_token("YOUR_TOKEN_HERE")` cell.

## cloudflared alternative (no account needed)

The notebook includes a commented-out cloudflared section. It doesn't need an
account but requires fetching the binary and parsing the tunnel URL from its
stdout.

## Automatic fallback

If the bridge is unreachable or returns errors on 3 consecutive batches, Smriti
falls back to local CPU embedding for the remainder of the job. The job
completes normally — just slower.

## Expected speed

| Library size | Local (i7-7567U) | T4 GPU | Speed-up |
|---|---|---|---|
| 10k photos (~30k faces) | ~5 min | ~30 s | 10× |
| 90k photos (~200k faces) | ~3 hrs | ~2.5 min | 70× |

## Troubleshooting

**Test connection says "Unreachable"**
- Check that the notebook is still running. Colab/Kaggle sessions time out
  after 30–90 min of inactivity.
- Verify the ngrok URL hasn't changed (free ngrok URLs rotate on restart).

**Embedding is slow — still on CPU**
- Check the Smriti logs (Help → Open logs folder). You should see
  "Remote GPU bridge at <url> is not healthy" if the bridge failed.
- Restart the notebook. Get a fresh ngrok URL. Update in Settings.

**ngrok free plan limits**
- 1 online tunnel at a time, 40 connections/min, 1 GB/month bandwidth.
  Face crops at 5 KB each × 200k faces = ~1 GB. For a 90k-photo library,
  this fits within the free plan but is near the limit.
- If you hit the limit, use cloudflared (no limits) or Colab Pro ($10/mo).

## Privacy notes

- Smriti sends **only** 112×112 face crops, not the original photos.
- No metadata (location, filenames, EXIF) is sent.
- No telemetry or analytics from Smriti itself.
- The bridge URL is set by you; Smriti never phones home.
- The notebook runs on your Kaggle/Colab account, not a shared service.

## Cost

All paths are free or nominal:
- **Kaggle**: Free (30 hrs/week GPU quota, resets weekly)
- **Colab free**: Free (limits apply; sessions may throttle)
- **Colab Pro**: $10/mo (A100 GPU, unmetered)
- **ngrok**: Free (1 online tunnel, 1 GB/month)
- **cloudflared**: Free (no account needed)
