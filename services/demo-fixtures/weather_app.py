#!/usr/bin/env python3
"""A real weather web app for the Hermes browser.

This is the first thing the browser is pointed at that is not part of the
demo fixture's own campaign machinery. It is an ordinary web app: a page,
and a small server-side API that proxies two real upstream services.

Upstream services, both free and key-less:

- `geocoding-api.open-meteo.com` turns a place name into coordinates.
- `api.open-meteo.com` turns coordinates into current conditions and a
  short forecast.

The proxy exists for one honest reason and not for convenience: the browser
page must make *same-origin* requests, because a page that calls a
cross-origin API directly needs CORS headers the upstream may not send, and
because the upstream should not see the browser's origin. The proxy is a
thin pass-through. It does not cache, does not reshape, and does not invent
any data: every field the page shows comes from the upstream response.

The page is **server-rendered**, and that is a deliberate fit to the engine
rather than a simplification of the app. The Blitz page runtime Hermes
builds on has no `fetch` and no `XMLHttpRequest`; a page cannot pull its own
data over the network. The Disco Workbench solves the same problem the same
way: the server fetches, the server renders, and the browser renders the
result it was given. The fixture's other pages work the same way.

So `/weather?place=NAME` is a complete, honest weather page for one place,
and choosing a place is a navigation. The JSON API is still exposed at
`/api/weather`, so the same real data is available to anything that can
speak HTTP.

Nothing here is mocked. If Open-Meteo is unreachable, the app says so and
shows the real reason.
"""

from __future__ import annotations

import html
import json
import urllib.error
import urllib.parse
import urllib.request

GEOCODE_URL = "https://geocoding-api.open-meteo.com/v1/search"
FORECAST_URL = "https://api.open-meteo.com/v1/forecast"

# A short request timeout. A weather page that hangs is worse than one that
# says the service is unreachable.
UPSTREAM_TIMEOUT_SECONDS = 8

# The places offered on the page. Each one is a real place that the real
# geocoder resolves; the coordinates are fetched, never hard-coded.
SUGGESTED_PLACES = [
    "San Francisco",
    "New York",
    "London",
    "Tokyo",
    "Sydney",
    "Reykjavík",
]

# WMO weather interpretation codes, which is what Open-Meteo reports. These
# are the standard's own code set, not a private scheme. Each code maps to a
# phrase and an icon *family*; the family decides which SVG is drawn.
WMO_CODES: dict[int, tuple[str, str]] = {
    0: ("Clear sky", "sun"),
    1: ("Mainly clear", "sun-cloud"),
    2: ("Partly cloudy", "sun-cloud"),
    3: ("Overcast", "cloud"),
    45: ("Fog", "fog"),
    46: ("Depositing rime fog", "fog"),
    47: ("Light drizzle", "drizzle"),
    48: ("Moderate drizzle", "drizzle"),
    51: ("Light drizzle", "drizzle"),
    53: ("Moderate drizzle", "drizzle"),
    55: ("Dense drizzle", "rain"),
    56: ("Light freezing drizzle", "rain"),
    57: ("Dense freezing drizzle", "rain"),
    61: ("Slight rain", "rain"),
    63: ("Moderate rain", "rain"),
    65: ("Heavy rain", "rain"),
    66: ("Light freezing rain", "rain"),
    67: ("Heavy freezing rain", "rain"),
    71: ("Slight snow fall", "snow"),
    73: ("Moderate snow fall", "snow"),
    75: ("Heavy snow fall", "snow"),
    77: ("Snow grains", "snow"),
    80: ("Slight rain showers", "showers"),
    81: ("Moderate rain showers", "showers"),
    82: ("Violent rain showers", "storm"),
    85: ("Slight snow showers", "snow"),
    86: ("Heavy snow showers", "snow"),
    95: ("Thunderstorm", "storm"),
    96: ("Thunderstorm with slight hail", "storm"),
    99: ("Thunderstorm with heavy hail", "storm"),
}

# Icon colours, chosen to read clearly on the page's dark card background.
_SUN = "#ffd60a"
_SUN_RING = "#ffb703"
_CLOUD = "#c9d1d9"
_CLOUD_DARK = "#8b949e"
_RAIN = "#58a6ff"
_SNOW = "#d8f0ff"
_BOLT = "#ffd60a"
_FOG = "#9aa4b2"


def _sun_rays() -> str:
    """Rays around the sun disc."""
    parts = []
    for index in range(8):
        angle = index * 45
        parts.append(
            f'<line x1="32" y1="4" x2="32" y2="12" stroke="{_SUN_RING}" '
            f'stroke-width="3.5" stroke-linecap="round" '
            f'transform="rotate({angle} 32 32)" />'
        )
    return "".join(parts)


def _cloud_body(fill: str = _CLOUD) -> str:
    """A single cloud silhouette."""
    return (
        f'<path d="M20 46 A10 10 0 0 1 22 27 A13 13 0 0 1 46 25 A9 9 0 0 1 47 46 Z" '
        f'fill="{fill}" />'
    )


def _raindrops(count: int = 3) -> str:
    """Rain strokes under a cloud."""
    parts = []
    xs = [22, 32, 42][:count]
    for index, x in enumerate(xs):
        offset = 0 if index % 2 == 0 else 3
        parts.append(
            f'<line x1="{x}" y1="48" x2="{x - 3}" y2="{58 + offset}" '
            f'stroke="{_RAIN}" stroke-width="3.5" stroke-linecap="round" />'
        )
    return "".join(parts)


def _snowflakes(count: int = 3) -> str:
    """Small six-point snowflakes under a cloud."""
    parts = []
    xs = [22, 32, 42][:count]
    for x in xs:
        parts.append(
            f'<g stroke="{_SNOW}" stroke-width="2" stroke-linecap="round">'
            f'<line x1="{x}" y1="49" x2="{x}" y2="59" />'
            f'<line x1="{x - 4}" y1="51.5" x2="{x + 4}" y2="56.5" />'
            f'<line x1="{x - 4}" y1="56.5" x2="{x + 4}" y2="51.5" />'
            f"</g>"
        )
    return "".join(parts)


def weather_svg(family: str, size: int = 56) -> str:
    """Draws one weather icon as inline SVG.

    Inline SVG rather than an emoji: the engine's font stack has no emoji
    font, so an emoji renders as a tofu box. Vector shapes render as drawn.
    """
    body = ""
    if family == "sun":
        body = f'<circle cx="32" cy="32" r="12" fill="{_SUN}" />{_sun_rays()}'
    elif family == "sun-cloud":
        body = (
            f'<circle cx="24" cy="22" r="9" fill="{_SUN}" />'
            f'<g stroke="{_SUN_RING}" stroke-width="3" stroke-linecap="round">'
            f'<line x1="24" y1="5" x2="24" y2="10" />'
            f'<line x1="9" y1="22" x2="14" y2="22" />'
            f'<line x1="13" y1="11" x2="17" y2="15" />'
            f'<line x1="35" y1="11" x2="31" y2="15" />'
            f"</g>"
            f"{_cloud_body(_CLOUD)}"
        )
    elif family == "cloud":
        body = _cloud_body(_CLOUD_DARK)
    elif family == "fog":
        body = (
            f"{_cloud_body(_CLOUD_DARK)}"
            f'<g stroke="{_FOG}" stroke-width="3.5" stroke-linecap="round">'
            f'<line x1="16" y1="50" x2="48" y2="50" />'
            f'<line x1="20" y1="57" x2="44" y2="57" />'
            f"</g>"
        )
    elif family == "drizzle":
        body = _cloud_body(_CLOUD_DARK) + _raindrops(2)
    elif family == "rain":
        body = _cloud_body(_CLOUD_DARK) + _raindrops(3)
    elif family == "showers":
        body = (
            f'<circle cx="24" cy="22" r="9" fill="{_SUN}" />'
            f"{_cloud_body(_CLOUD_DARK)}{_raindrops(3)}"
        )
    elif family == "snow":
        body = _cloud_body(_CLOUD_DARK) + _snowflakes(3)
    elif family == "storm":
        body = (
            f"{_cloud_body(_CLOUD_DARK)}"
            f'<path d="M33 46 L25 58 L31 58 L27 66 L38 53 L32 53 Z" fill="{_BOLT}" />'
        )
    else:
        body = f'<circle cx="32" cy="32" r="10" fill="{_CLOUD_DARK}" />'

    return (
        f'<svg class="wicon" viewBox="0 0 64 64" width="{size}" height="{size}" '
        f'xmlns="http://www.w3.org/2000/svg" role="img" aria-hidden="true">{body}</svg>'
    )


def describe_weather(code: int) -> tuple[str, str]:
    """Turn a WMO weather code into a human phrase and an icon family."""
    return WMO_CODES.get(code, ("Unknown", "unknown"))


def _fetch_json(url: str, params: dict[str, str]) -> dict:
    """Fetches JSON from an upstream service, or raises."""
    full = f"{url}?{urllib.parse.urlencode(params)}"
    request = urllib.request.Request(
        full,
        headers={
            # Identify the client honestly. Open-Meteo asks for a contact in
            # the User-Agent so an operator can reach a misbehaving client.
            "User-Agent": "HermesBrowserWeatherApp/1.0 (local demo fixture)",
            "Accept": "application/json",
        },
    )
    with urllib.request.urlopen(request, timeout=UPSTREAM_TIMEOUT_SECONDS) as response:
        if response.status != 200:
            raise RuntimeError(f"upstream returned HTTP {response.status}")
        return json.loads(response.read().decode("utf-8"))


def geocode(place: str) -> dict:
    """Resolves a place name to a location."""
    raw = _fetch_json(
        GEOCODE_URL,
        {
            "name": place,
            "count": "1",
            "language": "en",
            "format": "json",
        },
    )
    results = raw.get("results") or []
    if not results:
        raise LookupError(f"no place named {place!r}")
    hit = results[0]
    return {
        "name": hit.get("name") or place,
        "region": hit.get("admin1") or "",
        "country": hit.get("country") or "",
        "country_code": hit.get("country_code") or "",
        "latitude": hit.get("latitude"),
        "longitude": hit.get("longitude"),
        "timezone": hit.get("timezone") or "",
    }


def forecast(latitude: float, longitude: float) -> dict:
    """Fetches current conditions and a short daily forecast."""
    raw = _fetch_json(
        FORECAST_URL,
        {
            "latitude": str(latitude),
            "longitude": str(longitude),
            "current": "temperature_2m,apparent_temperature,weather_code,wind_speed_10m,relative_humidity_2m,is_day",
            "daily": "weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset",
            "timezone": "auto",
            "forecast_days": "5",
        },
    )
    current = raw.get("current") or {}
    daily = raw.get("daily") or {}
    days: list[dict] = []
    codes = daily.get("weather_code") or []
    highs = daily.get("temperature_2m_max") or []
    lows = daily.get("temperature_2m_min") or []
    sunrises = daily.get("sunrise") or []
    sunsets = daily.get("sunset") or []
    for index in range(len(codes)):
        code = int(codes[index])
        phrase, icon = describe_weather(code)
        days.append(
            {
                "code": code,
                "label": phrase,
                "icon": icon,
                "max": highs[index] if index < len(highs) else None,
                "min": lows[index] if index < len(lows) else None,
                "sunrise": sunrises[index] if index < len(sunrises) else "",
                "sunset": sunsets[index] if index < len(sunsets) else "",
            }
        )
    code = int(current.get("weather_code") or 0)
    phrase, icon = describe_weather(code)
    return {
        "current": {
            "code": code,
            "label": phrase,
            "icon": icon,
            "temperature": current.get("temperature_2m"),
            "apparent_temperature": current.get("apparent_temperature"),
            "wind_speed": current.get("wind_speed_10m"),
            "humidity": current.get("relative_humidity_2m"),
            "is_day": bool(current.get("is_day", 1)),
            "observed_at": current.get("time") or "",
        },
        "days": days,
        "timezone": raw.get("timezone") or "",
        "units": {
            "temperature": raw.get("current_units", {}).get("temperature_2m", "°C"),
            "wind": raw.get("current_units", {}).get("wind_speed_10m", "km/h"),
        },
    }


def weather_payload(place: str) -> dict:
    """The whole answer for one place: where it is and what it is doing."""
    location = geocode(place)
    report = forecast(float(location["latitude"]), float(location["longitude"]))
    return {"location": location, "forecast": report}


def _temperature(value: object, unit: str) -> str:
    """Formats a temperature for display, or a dash when there is none.

    Open-Meteo reports its temperature unit as `°C`, degree sign included,
    so the unit is appended verbatim rather than being given a degree sign
    of its own.
    """
    if value is None:
        return "—"
    try:
        return f"{float(value):.0f}{unit}"
    except (TypeError, ValueError):
        return "—"


def _number(value: object, suffix: str = "") -> str:
    if value is None:
        return "—"
    try:
        text = f"{float(value):.1f}"
    except (TypeError, ValueError):
        return "—"
    return text + suffix


def _wind(value: object, unit: str) -> str:
    if value is None:
        return "—"
    try:
        return f"{float(value):.0f} {unit}"
    except (TypeError, ValueError):
        return "—"


def _day_name(iso_timestamp: str, index: int) -> str:
    """A short weekday label for a daily entry."""
    if index == 0:
        return "Today"
    if index == 1:
        return "Tomorrow"
    # ISO 8601 local timestamps from Open-Meteo look like `2026-11-07T06:41`.
    day = iso_timestamp.split("T")[0] if iso_timestamp else ""
    try:
        year, month, day_number = (int(part) for part in day.split("-"))
    except (TypeError, ValueError):
        return f"Day {index + 1}"
    import datetime

    try:
        parsed = datetime.date(year, month, day_number)
    except ValueError:
        return f"Day {index + 1}"
    return parsed.strftime("%a")


def _place_link(place: str) -> str:
    return "/weather?place=" + urllib.parse.quote(place)


def render_weather_html(place: str = "San Francisco", error: str | None = None) -> str:
    """Renders the weather page for one place.

    The data is fetched here, on the server, and baked into the markup —
    the same shape the Disco Workbench uses. `error` is the real reason the
    upstream could not be reached, shown verbatim rather than hidden.
    """
    location: dict = {}
    report: dict = {}
    used_place = place

    if error is None:
        try:
            payload = weather_payload(place)
            location = payload["location"]
            report = payload["forecast"]
            used_place = location.get("name") or place
        except LookupError as exc:
            error = str(exc)
        except urllib.error.HTTPError as exc:
            error = f"the weather service returned HTTP {exc.code}"
        except urllib.error.URLError as exc:
            error = f"the weather service is unreachable: {exc.reason}"
        except TimeoutError:
            error = "the weather service timed out"
        except Exception as exc:  # noqa: BLE001 - the reason is reported, not swallowed
            error = f"the weather service failed: {exc}"

    escaped_place = html.escape(used_place)
    title = f"Hermes Weather — {escaped_place}" if error is None else "Hermes Weather"

    links_html = " ".join(
        f'<a class="chip" href="{html.escape(_place_link(candidate))}">'
        f"{html.escape(candidate)}</a>"
        for candidate in SUGGESTED_PLACES
    )

    if error is not None:
        body_html = f"""
    <div class="card error-card" id="weather-error">
      <div class="error-title">Could not load the weather</div>
      <div class="error-reason">{html.escape(error)}</div>
      <div class="error-hint">This is the real reason reported by the upstream service. Nothing has been invented.</div>
    </div>"""
    else:
        current = report.get("current") or {}
        units = report.get("units") or {}
        temp_unit = str(units.get("temperature") or "°C")
        wind_unit = str(units.get("wind") or "km/h")

        region = str(location.get("region") or "")
        country = str(location.get("country") or "")
        place_line = ", ".join(part for part in [region, country] if part)

        facts_html = " ".join(
            f"<div class=\"fact\">{html.escape(label)} <b>{html.escape(value)}</b></div>"
            for label, value in [
                ("Feels like", _temperature(current.get("apparent_temperature"), temp_unit)),
                ("Wind", _wind(current.get("wind_speed"), wind_unit)),
                ("Humidity", _number(current.get("humidity"), "%")),
                ("Observed", str(current.get("observed_at") or "—")),
            ]
        )

        days_html = ""
        for index, day in enumerate(report.get("days") or []):
            days_html += f"""
      <div class="day">
        <div class="dg">{weather_svg(str(day.get("icon") or "unknown"), 40)}</div>
        <div class="dd">{html.escape(_day_name(str(day.get("sunrise") or ""), index))}</div>
        <div class="dl">{html.escape(str(day.get("label") or ""))}</div>
        <div class="dt">{html.escape(_temperature(day.get("max"), temp_unit))} / {html.escape(_temperature(day.get("min"), temp_unit))}</div>
      </div>"""

        body_html = f"""
    <div class="card">
      <div class="now">
        <div class="glyph" id="w-icon">{weather_svg(str(current.get("icon") or "unknown"), 72)}</div>
        <div class="nowtext">
          <div class="temp" id="w-temp">{html.escape(_temperature(current.get("temperature"), temp_unit))}</div>
          <div class="label" id="w-label">{html.escape(str(current.get("label") or ""))}</div>
          <div class="place" id="w-place">{html.escape(str(location.get("name") or used_place))}{(" · " + html.escape(place_line)) if place_line else ""}</div>
        </div>
      </div>
      <div class="facts">{facts_html}</div>
    </div>
    <div class="card">
      <div class="days" id="w-days">{days_html}
      </div>
    </div>"""

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>{html.escape(title)}</title>
<style>
  body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
    margin: 0; padding: 24px;
    background: #0d1117; color: #e6edf3;
  }}
  .wrap {{ max-width: 760px; margin: 0 auto; }}
  h1 {{ font-size: 22px; margin: 0 0 4px 0; }}
  .sub {{ color: #8b949e; font-size: 13px; margin: 0 0 18px 0; }}
  .picker {{ margin-bottom: 18px; }}
  .chips {{ display: flex; gap: 8px; flex-wrap: wrap; }}
  .chip {{
    display: inline-block; padding: 6px 12px; font-size: 13px;
    border: 1px solid #30363d; border-radius: 14px;
    background: #161b22; color: #c9d1d9; text-decoration: none;
  }}
  .card {{
    background: #161b22; border: 1px solid #30363d; border-radius: 10px;
    padding: 18px 20px; margin-bottom: 14px;
  }}
  .now {{ display: flex; align-items: center; gap: 20px; }}
  .glyph {{ line-height: 1; }}
  .glyph svg {{ display: block; }}
  .temp {{ font-size: 44px; font-weight: 700; }}
  .label {{ font-size: 16px; color: #c9d1d9; }}
  .place {{ font-size: 15px; color: #8b949e; margin-top: 2px; }}
  .facts {{ display: flex; gap: 22px; margin-top: 14px; font-size: 13px; color: #8b949e; }}
  .facts b {{ color: #e6edf3; font-weight: 600; }}
  .days {{ display: flex; gap: 10px; flex-wrap: wrap; }}
  .day {{
    flex: 1 1 120px; text-align: center; padding: 12px 8px;
    background: #0d1117; border: 1px solid #21262d; border-radius: 8px;
  }}
  .day .dg {{ line-height: 1; }}
  .day .dg svg {{ display: block; margin: 0 auto; }}
  .day .dd {{ font-size: 12px; color: #8b949e; margin-top: 4px; }}
  .day .dl {{ font-size: 12px; color: #c9d1d9; margin-top: 2px; }}
  .day .dt {{ font-size: 14px; margin-top: 6px; font-weight: 600; }}
  .error-title {{ color: #ff7b72; font-size: 16px; font-weight: 600; }}
  .error-reason {{ color: #ffa198; font-size: 14px; margin-top: 6px; }}
  .error-hint {{ color: #8b949e; font-size: 12px; margin-top: 8px; }}
  .src {{ color: #6e7681; font-size: 12px; margin-top: 18px; }}
  .src a {{ color: #58a6ff; }}
</style>
</head>
<body>
  <div class="wrap">
    <h1>Hermes Weather</h1>
    <p class="sub">Real observations from Open-Meteo. Nothing on this page is simulated.</p>

    <div class="picker">
      <div class="chips">{links_html}</div>
    </div>
{body_html}
    <div class="src">Source: <a href="https://open-meteo.com/">open-meteo.com</a> — geocoding and forecast, served live at request time.</div>
  </div>
</body>
</html>"""


def weather_api_json(place: str) -> tuple[int, dict]:
    """Serves one weather request as JSON.

    Returns a status and a JSON body. Upstream failures are reported as a
    502 with the reason, never as a made-up forecast.
    """
    place = (place or "").strip()
    if not place:
        return 400, {"error": "a place name is required"}
    try:
        return 200, weather_payload(place)
    except LookupError as error:
        return 404, {"error": str(error)}
    except urllib.error.HTTPError as error:
        return 502, {"error": f"the weather service returned HTTP {error.code}"}
    except urllib.error.URLError as error:
        return 502, {"error": f"the weather service is unreachable: {error.reason}"}
    except TimeoutError:
        return 504, {"error": "the weather service timed out"}
    except Exception as error:  # noqa: BLE001 - the reason is reported, not swallowed
        return 502, {"error": f"the weather service failed: {error}"}
