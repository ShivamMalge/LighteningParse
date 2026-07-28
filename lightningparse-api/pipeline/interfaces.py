from abc import ABC, abstractmethod
from langchain_core.language_models.chat_models import BaseChatModel
from langchain_core.embeddings import Embeddings

class ModelProvider(ABC):
    """
    Swappable interface for LLMs and Embeddings.
    Allows easy substitution of OpenAI with local providers (e.g. Ollama, HuggingFace).
    """
    
    @abstractmethod
    def get_llm(self) -> BaseChatModel:
        pass
        
    @abstractmethod
    def get_embeddings(self) -> Embeddings:
        pass
