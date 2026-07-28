from typing import List
from langchain_core.documents import Document
from langchain_chroma import Chroma
from langchain_core.vectorstores import VectorStoreRetriever
from .interfaces import ModelProvider

class DocumentRetriever:
    """
    Initializes a Chroma vector store from a list of documents and returns a LangChain Retriever.
    """
    
    def __init__(self, provider: ModelProvider):
        self.embeddings = provider.get_embeddings()

    def build_retriever(self, documents: List[Document]) -> VectorStoreRetriever:
        """
        Builds an in-memory Chroma vector store and returns a retriever.
        """
        # We use an ephemeral in-memory Chroma instance for simplicity per request.
        vectorstore = Chroma.from_documents(
            documents=documents, 
            embedding=self.embeddings
        )
        return vectorstore.as_retriever(search_kwargs={"k": 4})
