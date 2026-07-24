#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttributeTargetKind {
    TypeDeclaration,
    EnumDeclaration,
    ContractDeclaration,
    ContractMethodDeclaration,
    ModuleDeclaration,
    FunctionDeclaration,
    MethodDeclaration,
    FieldDeclaration,
    ParameterDeclaration,
}

impl AttributeTargetKind {
    pub const ALL: [Self; 9] = [
        Self::TypeDeclaration,
        Self::EnumDeclaration,
        Self::ContractDeclaration,
        Self::ContractMethodDeclaration,
        Self::ModuleDeclaration,
        Self::FunctionDeclaration,
        Self::MethodDeclaration,
        Self::FieldDeclaration,
        Self::ParameterDeclaration,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TypeDeclaration => "TypeDeclaration",
            Self::EnumDeclaration => "EnumDeclaration",
            Self::ContractDeclaration => "ContractDeclaration",
            Self::ContractMethodDeclaration => "ContractMethodDeclaration",
            Self::ModuleDeclaration => "ModuleDeclaration",
            Self::FunctionDeclaration => "FunctionDeclaration",
            Self::MethodDeclaration => "MethodDeclaration",
            Self::FieldDeclaration => "FieldDeclaration",
            Self::ParameterDeclaration => "ParameterDeclaration",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "TypeDeclaration" | "TypeDefinition" => Some(Self::TypeDeclaration),
            "EnumDeclaration" => Some(Self::EnumDeclaration),
            "ContractDeclaration" => Some(Self::ContractDeclaration),
            "ContractMethodDeclaration" | "ContractMethodSignature" => Some(Self::ContractMethodDeclaration),
            "ModuleDeclaration" => Some(Self::ModuleDeclaration),
            "FunctionDeclaration" => Some(Self::FunctionDeclaration),
            "MethodDeclaration" | "MethodDefinition" => Some(Self::MethodDeclaration),
            "FieldDeclaration" | "Field" => Some(Self::FieldDeclaration),
            "ParameterDeclaration" => Some(Self::ParameterDeclaration),
            _ => None,
        }
    }
}
