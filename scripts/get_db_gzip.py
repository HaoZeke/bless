import click
from pymongo import MongoClient
from bson.binary import Binary

@click.command()
@click.option('--mongo-url', default='mongodb://localhost:27017', help='MongoDB connection URL', show_default=True)
@click.option('--db-name', required=True, help='Database name')
@click.option('--collection-name', required=True, help='Collection name')
@click.option('--query-field', required=True, help='Field to query')
@click.option('--query-value', required=True, help='Value to query')
@click.option('--output-file', default='output.gzip', help='Output file name', show_default=True)
def write_gzip_blob(mongo_url, db_name, collection_name, query_field, query_value, output_file):
    """
    A simple CLI tool to retrieve and save gzip_blob from a MongoDB document to a .gzip file.
    """
    client = MongoClient(mongo_url)
    try:
        db = client[db_name]
        collection = db[collection_name]
        query = {query_field: query_value}
        document = collection.find_one(query)

        if document:
            if "gzip_blob" in document:
                gzip_blob = document["gzip_blob"]
                # Write the binary data to a .gzip file if it is a binary type
                if isinstance(gzip_blob, (Binary, bytes)):
                    with open(output_file, "wb") as file:
                        file.write(gzip_blob)
                    click.echo(f"File written successfully: {output_file}")
                else:
                    click.echo("gzip_blob field is not of type Binary or bytes")
            else:
                click.echo("gzip_blob field is missing")
        else:
            click.echo("Document not found")
    except Exception as e:
        click.echo(f"Error: {e}")
    finally:
        client.close()

if __name__ == "__main__":
    write_gzip_blob()
