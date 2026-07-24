use tower_lsp_server::ls_types::{
    CompletionParams, GotoDefinitionParams, HoverParams, ReferenceParams, RenameParams, SignatureHelpParams,
    TextDocumentPositionParams,
};

pub trait IntoTextDocumentPosition {
    fn into_text_document_position(self) -> TextDocumentPositionParams;
}

impl IntoTextDocumentPosition for TextDocumentPositionParams {
    fn into_text_document_position(self) -> TextDocumentPositionParams {
        self
    }
}

impl IntoTextDocumentPosition for HoverParams {
    fn into_text_document_position(self) -> TextDocumentPositionParams {
        self.text_document_position_params
    }
}

impl IntoTextDocumentPosition for GotoDefinitionParams {
    fn into_text_document_position(self) -> TextDocumentPositionParams {
        self.text_document_position_params
    }
}

impl IntoTextDocumentPosition for ReferenceParams {
    fn into_text_document_position(self) -> TextDocumentPositionParams {
        self.text_document_position
    }
}

impl IntoTextDocumentPosition for CompletionParams {
    fn into_text_document_position(self) -> TextDocumentPositionParams {
        self.text_document_position
    }
}

impl IntoTextDocumentPosition for SignatureHelpParams {
    fn into_text_document_position(self) -> TextDocumentPositionParams {
        self.text_document_position_params
    }
}

impl IntoTextDocumentPosition for RenameParams {
    fn into_text_document_position(self) -> TextDocumentPositionParams {
        self.text_document_position
    }
}
