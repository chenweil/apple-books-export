# knowledge/provider/base.py
"""Abstract base class for LLM providers."""


class LLMProvider:
    """Base LLM provider interface."""

    def complete(self, prompt: str, system: str = "") -> str:
        """Send a prompt and return the LLM's text response."""
        raise NotImplementedError

    def batch_complete(self, prompts: list[str], system: str = "") -> list[str]:
        """Send multiple prompts and return responses. Default: sequential."""
        return [self.complete(p, system) for p in prompts]
