import os
from langchain_openai import ChatOpenAI, OpenAIEmbeddings
from langchain_core.language_models.chat_models import BaseChatModel
from langchain_core.embeddings import Embeddings
from .interfaces import ModelProvider

class OpenAIProvider(ModelProvider):
    """
    OpenAI implementation for LLMs and Embeddings.
    Requires OPENAI_API_KEY environment variable.
    """
    
    def __init__(self, model_name: str = "gpt-4o-mini", embedding_model: str = "text-embedding-3-small"):
        self.model_name = model_name
        self.embedding_model = embedding_model
        
        if not os.getenv("OPENAI_API_KEY"):
            raise ValueError("OPENAI_API_KEY environment variable not set. Please set it to use OpenAIProvider.")

    def get_llm(self) -> BaseChatModel:
        return ChatOpenAI(model=self.model_name, temperature=0)

    def get_embeddings(self) -> Embeddings:
        return OpenAIEmbeddings(model=self.embedding_model)
