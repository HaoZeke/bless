project = "bless"
copyright = "2024--present, Rohit Goswami"
author = "Rohit Goswami"
release = "0.2.0"
html_logo = "../../branding/logo/bless_logo.png"

extensions = [
    "myst_parser",
    "sphinx_sitemap",
    "sphinx.ext.intersphinx",
]

templates_path = ["_templates"]
exclude_patterns = []

html_theme = "shibuya"
html_static_path = ["_static"]
html_baseurl = "https://bless.rgoswami.me/"

html_theme_options = {
    "accent_color": "teal",
    "dark_code": True,
    "globaltoc_expand_depth": 2,
    "github_url": "https://github.com/HaoZeke/bless",
    "nav_links": [
        {"title": "rgoswami.me", "url": "https://rgoswami.me"},
    ],
}

html_context = {
    "source_type": "github",
    "source_user": "HaoZeke",
    "source_repo": "bless",
    "source_version": "main",
    "source_docs_path": "/docs/source/",
}

myst_enable_extensions = [
    "colon_fence",
    "deflist",
]

intersphinx_mapping = {}

source_suffix = {
    ".rst": "restructuredtext",
    ".md": "markdown",
}
