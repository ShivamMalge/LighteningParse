from typing import Any, Dict
from langchain_classic.chains import create_retrieval_chain
from langchain_classic.chains.combine_documents import create_stuff_documents_chain
from langchain_core.prompts import ChatPromptTemplate
from langchain_core.vectorstores import VectorStoreRetriever
from .interfaces import ModelProvider

class QAChain:
    """
    Constructs the end-to-end Retrieval-Augmented Generation chain.
    """
    
    def __init__(self, provider: ModelProvider, retriever: VectorStoreRetriever):
        self.llm = provider.get_llm()
        self.retriever = retriever
        
        system_prompt = (
            "You are an assistant for question-answering tasks. "
            "Use the following pieces of retrieved context to answer the question. "
            "If you don't know the answer, say that you don't know. "
            "Context:\n\n{context}"
        )
        
        self.prompt = ChatPromptTemplate.from_messages([
            ("system", system_prompt),
            ("human", "{input}"),
        ])
        
        self.question_answer_chain = create_stuff_documents_chain(self.llm, self.prompt)
        self.rag_chain = create_retrieval_chain(self.retriever, self.question_answer_chain)

    def invoke(self, question: str) -> Dict[str, Any]:
        """
        Executes the QA chain. Returns a dictionary containing 'answer' and 'context'.
        """
        response = self.rag_chain.invoke({"input": question})
        return {
            "answer": response["answer"],
            "context": response["context"]  # these are the Document chunks retrieved
        }
