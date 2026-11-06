"""Bounded retries for read-only startup health checks, never order submission."""
import time
import urllib.error


def wait_ready(get_health, attempts=180, delay=1):
    if attempts < 1:
        raise ValueError("attempts must be positive")
    last = "not ready"
    for attempt in range(attempts):
        try:
            health = get_health()
            if health["status"] == "ready":
                return health
            last = "not ready"
        except urllib.error.HTTPError as error:
            if not 500 <= error.code < 600:
                raise
            last = f"HTTP {error.code}"
        except (urllib.error.URLError, ConnectionError, TimeoutError) as error:
            last = type(error).__name__
        if attempt + 1 < attempts:
            time.sleep(delay)
    raise SystemExit(f"Engine did not become ready within {attempts} attempts ({last})")
