class OfffError(Exception):
    """Base error for OFFF SDK."""


class ValidationError(OfffError):
    """Raised when container data fails integrity checks."""


class UnsupportedVersionError(OfffError):
    """Raised when the container OFFF version is unsupported by this SDK."""
