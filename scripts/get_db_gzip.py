import click
from pymongo import MongoClient
from bson.binary import Binary
from bson.objectid import ObjectId
import gridfs

def get_mongo_client(mongo_url):
    """
    Get a MongoDB client.
    """
    return MongoClient(mongo_url)

def get_document(collection, query):
    """
    Retrieve the document from MongoDB based on the query.
    """
    return collection.find_one(query)

def write_binary_to_file(binary_data, output_file):
    """
    Write binary data to a file.
    """
    with open(output_file, "wb") as file:
        file.write(binary_data)
    click.echo(f"File written successfully: {output_file}")

def download_from_gridfs(db, file_id, output_file):
    """
    Download a file from GridFS using the file ID and write it to a file.
    """
    fs = gridfs.GridFS(db)
    with fs.get(file_id) as grid_out:
        with open(output_file, "wb") as file:
            file.write(grid_out.read())
    click.echo(f"File written successfully from GridFS: {output_file}")

def process_document(db, document, output_file):
    """
    Process the document to retrieve and write gzip_blob or download gzip_blob_id from GridFS.
    """
    if "gzip_blob" in document:
        gzip_blob = document["gzip_blob"]
        if isinstance(gzip_blob, (bytes, Binary)):
            write_binary_to_file(gzip_blob, output_file)
        else:
            raise TypeError("gzip_blob field is not of type Binary or bytes")
    elif "gzip_blob_id" in document:
        file_id = document["gzip_blob_id"]
        if isinstance(file_id, ObjectId):
            download_from_gridfs(db, file_id, output_file)
        else:
            raise TypeError("gzip_blob_id field is not of type ObjectId")
    else:
        raise ValueError("Both gzip_blob and gzip_blob_id fields are missing")

@click.command()
@click.option('--mongo-url', default='mongodb://localhost:27017', help='MongoDB connection URL', show_default=True)
@click.option('--db-name', required=True, help='Database name')
@click.option('--collection-name', required=True, help='Collection name')
@click.option('--query-field', required=True, help='Field to query')
@click.option('--query-value', required=True, help='Value to query')
@click.option('--output-file', default='output.gz', help='Output file name', show_default=True)
def write_gzip_blob(mongo_url, db_name, collection_name, query_field, query_value, output_file):
    """
    A simple CLI tool to retrieve and save gzip_blob or gzip_blob_id from a MongoDB document to a .gzip file.
    """
    try:
        client = get_mongo_client(mongo_url)
        db = client[db_name]
        collection = db[collection_name]
        query = {query_field: query_value}

        document = get_document(collection, query)
        if document:
            process_document(db, document, output_file)
        else:
            click.echo("Document not found")
    except Exception as e:
        click.echo(f"Error: {e}")
    finally:
        client.close()

if __name__ == "__main__":
    write_gzip_blob()
